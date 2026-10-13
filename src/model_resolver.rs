//! Bounded, explicit offline model asset lookup. Resolution captures bytes and
//! their hash; it does not parse geometry or certify a model's engineering use.
//! Standard filesystem checks reduce accidental escapes and mutation, but are
//! not an adversarial race-proof filesystem sandbox.
use crate::document::{ModelReference, SourceLocator};
use crate::{EngineError, sha256_hex};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_ROOTS: usize = 64;
const MAX_VARIABLES: usize = 64;
const MAX_PATH_BYTES: usize = 4096;
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ModelResolverConfig {
    pub project_root: PathBuf,
    /// Exact relative paths are checked in each root, without recursive search.
    pub library_roots: Vec<PathBuf>,
    pub cache_roots: Vec<PathBuf>,
    /// Directory mappings must be within an approved root. KIPRJMOD is built in.
    pub directory_variables: BTreeMap<String, PathBuf>,
    pub max_file_bytes: u64,
}
impl ModelResolverConfig {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            library_roots: Vec::new(),
            cache_roots: Vec::new(),
            directory_variables: BTreeMap::new(),
            max_file_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAssetFormat {
    Step,
    Wrl,
    Glb,
    Gltf,
}
impl ModelAssetFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::Wrl => "wrl",
            Self::Glb => "glb",
            Self::Gltf => "gltf",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelResolutionStatus {
    Resolved,
    Hidden,
    MissingPath,
    Missing,
    InvalidPath,
    UnknownVariable,
    UnsupportedFormat,
    TooLarge,
    IoError,
    Ambiguous,
    OutsideApprovedRoots,
    ChangedDuringRead,
}
impl ModelResolutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Hidden => "hidden",
            Self::MissingPath => "missing_path",
            Self::Missing => "missing",
            Self::InvalidPath => "invalid_path",
            Self::UnknownVariable => "unknown_variable",
            Self::UnsupportedFormat => "unsupported_format",
            Self::TooLarge => "too_large",
            Self::IoError => "io_error",
            Self::Ambiguous => "ambiguous",
            Self::OutsideApprovedRoots => "outside_approved_roots",
            Self::ChangedDuringRead => "changed_during_read",
        }
    }
}
#[derive(Debug, Clone)]
pub struct ResolvedModelAsset {
    format: ModelAssetFormat,
    canonical_path: PathBuf,
    bytes: Vec<u8>,
    sha256: String,
}
impl ResolvedModelAsset {
    pub fn format(&self) -> ModelAssetFormat {
        self.format
    }
    /// Local API only: this path must not be copied into shared manifests.
    pub fn local_path(&self) -> &Path {
        &self.canonical_path
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}
#[derive(Debug, Clone)]
pub struct ModelResolution {
    pub source: SourceLocator,
    pub status: ModelResolutionStatus,
    pub asset: Option<ResolvedModelAsset>,
    /// Static diagnostic text contains no raw filesystem or model path.
    pub diagnostic: Option<&'static str>,
}
impl ModelResolution {
    /// Shareable inventory. Source path is the numeric source-list locator,
    /// never a filesystem path. Resolved means asset lookup, not mesh support.
    pub fn shared_projection(&self) -> serde_json::Value {
        let asset = self.asset.as_ref();
        serde_json::json!({
            "source": { "kind": self.source.kind, "ordinal": self.source.ordinal,
                "source_uuid": self.source.source_uuid.as_deref(),
                "source_path": self.source.source_path,
                "component_ordinal": self.source.component_ordinal },
            "status": self.status.as_str(), "diagnostic": self.diagnostic,
            "format": asset.map(|a| a.format.as_str()),
            "byte_length": asset.map(|a| a.bytes.len()),
            "sha256": asset.map(|a| a.sha256.as_str()),
        })
    }
}
#[derive(Debug)]
pub struct ModelResolver {
    roots: Vec<PathBuf>,
    variables: BTreeMap<String, PathBuf>,
    max_file_bytes: u64,
}
type LookupError = (ModelResolutionStatus, &'static str);

impl ModelResolver {
    pub fn new(config: ModelResolverConfig) -> Result<Self, EngineError> {
        if config.library_roots.len() + config.cache_roots.len() + 1 > MAX_ROOTS
            || config.directory_variables.len() > MAX_VARIABLES
            || config.max_file_bytes == 0
            || config.max_file_bytes > MAX_ASSET_BYTES
        {
            return Err(EngineError::new(
                "Model resolver configuration exceeds bounded limits",
            ));
        }
        let project = approved_directory(&config.project_root)?;
        let mut roots = vec![project.clone()];
        for root in config.library_roots.iter().chain(&config.cache_roots) {
            let root = approved_directory(root)?;
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
        let mut variables = BTreeMap::from([("KIPRJMOD".to_owned(), project)]);
        for (name, path) in config.directory_variables {
            if name == "KIPRJMOD"
                || name.is_empty()
                || name.len() > 128
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(EngineError::new("Invalid model directory variable name"));
            }
            let path = approved_directory(&path)?;
            if !roots.iter().any(|root| within_root(&path, root)) {
                return Err(EngineError::new(
                    "Model directory variable is outside approved roots",
                ));
            }
            variables.insert(name, path);
        }
        Ok(Self {
            roots,
            variables,
            max_file_bytes: config.max_file_bytes,
        })
    }

    pub fn resolve(&self, model: &ModelReference) -> ModelResolution {
        let result = if model.hidden {
            Err((ModelResolutionStatus::Hidden, "Model reference is hidden"))
        } else {
            self.resolve_asset(model.path.as_deref())
        };
        match result {
            Ok(asset) => ModelResolution {
                source: model.source.clone(),
                status: ModelResolutionStatus::Resolved,
                asset: Some(asset),
                diagnostic: None,
            },
            Err((status, diagnostic)) => ModelResolution {
                source: model.source.clone(),
                status,
                asset: None,
                diagnostic: Some(diagnostic),
            },
        }
    }

    /// Capture an explicitly selected asset without inventing a PCB source node.
    pub fn resolve_selected_path(&self, path: &str) -> Result<ResolvedModelAsset, EngineError> {
        self.resolve_asset(Some(path))
            .map_err(|(status, diagnostic)| {
                EngineError::new(format!("{}: {diagnostic}", status.as_str()))
            })
    }

    fn resolve_asset(&self, raw: Option<&str>) -> Result<ResolvedModelAsset, LookupError> {
        let raw = raw.ok_or((ModelResolutionStatus::MissingPath, "Model path is absent"))?;
        validate_text_path(raw)?;
        let expanded = self.expand(raw)?;
        // Canonical variable roots may have Windows extended-local prefixes;
        // their trust was established during configuration, before expansion.
        let path = PathBuf::from(expanded.replace('\\', "/"));
        reject_traversal(&path)?;
        #[cfg(not(windows))]
        if path.to_string_lossy().as_bytes().get(1) == Some(&b':') {
            return Err((
                ModelResolutionStatus::InvalidPath,
                "Foreign platform absolute path",
            ));
        }
        let format = match path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("step" | "stp") => ModelAssetFormat::Step,
            Some("wrl" | "vrml") => ModelAssetFormat::Wrl,
            Some("glb") => ModelAssetFormat::Glb,
            Some("gltf") => ModelAssetFormat::Gltf,
            _ => {
                return Err((
                    ModelResolutionStatus::UnsupportedFormat,
                    "Unsupported model asset extension",
                ));
            }
        };
        let candidates = if path.is_absolute() {
            vec![path]
        } else {
            self.roots.iter().map(|root| root.join(&path)).collect()
        };
        let mut found = BTreeSet::new();
        for candidate in candidates {
            if !self.roots.iter().any(|root| within_root(&candidate, root)) {
                return Err((
                    ModelResolutionStatus::OutsideApprovedRoots,
                    "Model path is outside approved roots",
                ));
            }
            match check_chain(&candidate) {
                Ok(()) => {}
                Err(e) if e.0 == ModelResolutionStatus::Missing => continue,
                Err(e) => return Err(e),
            }
            let canonical = canonical_local(&candidate).map_err(io_error)?;
            if !self.roots.iter().any(|root| within_root(&canonical, root)) {
                return Err((
                    ModelResolutionStatus::OutsideApprovedRoots,
                    "Model path is outside approved roots",
                ));
            }
            found.insert(canonical);
        }
        if found.len() > 1 {
            return Err((
                ModelResolutionStatus::Ambiguous,
                "Multiple distinct exact model matches",
            ));
        }
        let canonical = found
            .into_iter()
            .next()
            .ok_or((ModelResolutionStatus::Missing, "Model asset is missing"))?;
        let path_metadata = fs::metadata(&canonical).map_err(io_error)?;
        if !path_metadata.is_file() {
            return Err((
                ModelResolutionStatus::InvalidPath,
                "Model asset is not a regular file",
            ));
        }
        let mut file = File::open(&canonical).map_err(io_error)?;
        let before = file.metadata().map_err(io_error)?;
        if !before.is_file() {
            return Err((
                ModelResolutionStatus::InvalidPath,
                "Model asset is not a regular file",
            ));
        }
        if before.len() > self.max_file_bytes {
            return Err((
                ModelResolutionStatus::TooLarge,
                "Model asset exceeds byte limit",
            ));
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(self.max_file_bytes + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() as u64 > self.max_file_bytes {
            return Err((
                ModelResolutionStatus::TooLarge,
                "Model asset exceeds byte limit",
            ));
        }
        let after = file.metadata().map_err(io_error)?;
        check_chain(&canonical)?;
        let current = fs::metadata(&canonical).map_err(io_error)?;
        if !same_metadata(&before, &after)
            || !same_metadata(&after, &current)
            || after.len() != bytes.len() as u64
            || canonical_local(&canonical).map_err(io_error)? != canonical
        {
            return Err((
                ModelResolutionStatus::ChangedDuringRead,
                "Model asset changed during bounded capture",
            ));
        }
        let sha256 = sha256_hex(&bytes);
        Ok(ResolvedModelAsset {
            format,
            canonical_path: canonical,
            bytes,
            sha256,
        })
    }

    fn expand(&self, raw: &str) -> Result<String, LookupError> {
        let mut remaining = raw;
        let mut result = String::new();
        let mut substitutions = 0;
        while let Some(index) = remaining.find('$') {
            result.push_str(&remaining[..index]);
            let token = &remaining[index..];
            if !token.starts_with("${") {
                return Err((
                    ModelResolutionStatus::InvalidPath,
                    "Malformed model variable",
                ));
            }
            let end = token.find('}').ok_or((
                ModelResolutionStatus::InvalidPath,
                "Malformed model variable",
            ))?;
            let name = &token[2..end];
            if name.is_empty()
                || name.len() > 128
                || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err((
                    ModelResolutionStatus::InvalidPath,
                    "Malformed model variable identifier",
                ));
            }
            let value = self.variables.get(name).ok_or((
                ModelResolutionStatus::UnknownVariable,
                "Model directory variable is not configured",
            ))?;
            // Normalize Windows's canonical local-path prefix for path joining.
            let text = value.to_str().ok_or((
                ModelResolutionStatus::InvalidPath,
                "Directory mapping is not UTF-8",
            ))?;
            result.push_str(text.strip_prefix("\\\\?\\").unwrap_or(text));
            remaining = &token[end + 1..];
            substitutions += 1;
            if substitutions > MAX_VARIABLES || result.len() > MAX_PATH_BYTES {
                return Err((
                    ModelResolutionStatus::InvalidPath,
                    "Expanded model path exceeds bounded limits",
                ));
            }
        }
        result.push_str(remaining);
        if result.len() > MAX_PATH_BYTES {
            return Err((
                ModelResolutionStatus::InvalidPath,
                "Expanded model path exceeds bounded limits",
            ));
        }
        Ok(result)
    }
}

fn validate_text_path(text: &str) -> Result<(), LookupError> {
    let normalized = text.replace('\\', "/");
    if text.is_empty()
        || text.len() > MAX_PATH_BYTES
        || text.chars().any(char::is_control)
        || normalized.starts_with("//")
        || normalized.contains("://")
        || normalized.split('/').any(|part| {
            part == ".."
                || windows_device_name(part)
                || (part != "." && (part.ends_with(' ') || part.ends_with('.')))
        })
        || normalized.contains(':')
            && !(normalized.bytes().filter(|b| *b == b':').count() == 1
                && normalized.as_bytes().get(1) == Some(&b':')
                && normalized
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
                && normalized.as_bytes().get(2) == Some(&b'/'))
    {
        return Err((
            ModelResolutionStatus::InvalidPath,
            "Invalid, traversal, network, or device model path",
        ));
    }
    Ok(())
}
fn reject_traversal(path: &Path) -> Result<(), LookupError> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err((
            ModelResolutionStatus::InvalidPath,
            "Model path contains traversal",
        ));
    }
    Ok(())
}
fn approved_directory(path: &Path) -> Result<PathBuf, EngineError> {
    let bad = || {
        EngineError::new(
            "Model root must be an existing absolute local directory without symlinks or reparse points",
        )
    };
    validate_text_path(path.to_str().ok_or_else(bad)?).map_err(|_| bad())?;
    if !path.is_absolute() {
        return Err(bad());
    }
    check_chain(path).map_err(|_| bad())?;
    let canonical = canonical_local(path).map_err(|_| bad())?;
    if !canonical.is_dir() {
        return Err(bad());
    }
    Ok(canonical)
}
fn check_chain(path: &Path) -> Result<(), LookupError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        let metadata = fs::symlink_metadata(&current).map_err(io_error)?;
        if metadata.file_type().is_symlink() || is_reparse(&metadata) {
            return Err((
                ModelResolutionStatus::InvalidPath,
                "Symlink or reparse point in model path",
            ));
        }
    }
    Ok(())
}
#[cfg(windows)]
fn is_reparse(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &Metadata) -> bool {
    false
}
fn same_metadata(a: &Metadata, b: &Metadata) -> bool {
    let common = a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
        && a.created().ok() == b.created().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        common && a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        common
    }
}
fn io_error(error: std::io::Error) -> LookupError {
    if error.kind() == std::io::ErrorKind::NotFound {
        (ModelResolutionStatus::Missing, "Model asset is missing")
    } else {
        (
            ModelResolutionStatus::IoError,
            "Local model asset I/O failed",
        )
    }
}

fn canonical_local(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        if let Some(text) = canonical.to_str().and_then(|s| s.strip_prefix("\\\\?\\")) {
            return Ok(PathBuf::from(text));
        }
    }
    Ok(canonical)
}
fn windows_device_name(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CLOCK$"
            | "CONIN$"
            | "CONOUT$"
            | "COM¹"
            | "COM²"
            | "COM³"
            | "LPT¹"
            | "LPT²"
            | "LPT³"
    ) || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.as_bytes()[3].is_ascii_digit())
}

/// Fail closed on differently spelled path components. Windows case aliases
/// can be rejected even on case-insensitive volumes; directory case sensitivity
/// must never make a distinct sibling qualify as an approved root.
fn within_root(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::within_root;
    use std::path::Path;

    #[test]
    fn approved_root_requires_exact_component_spelling() {
        assert!(within_root(
            Path::new("Project/models/unit.step"),
            Path::new("Project")
        ));
        assert!(!within_root(
            Path::new("project/models/unit.step"),
            Path::new("Project")
        ));
        assert!(!within_root(
            Path::new("ProjectOther/unit.step"),
            Path::new("Project")
        ));
    }
}
