//! Synthetic byte assets exercise lookup/security only, never mesh correctness.
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokito_cli::document::{ModelReference, SourceLocator};
use tokito_cli::model_resolver::{
    ModelAssetFormat, ModelResolutionStatus as Status, ModelResolver, ModelResolverConfig,
};

struct Fixture {
    root: PathBuf,
    project: PathBuf,
    library: PathBuf,
    outside: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/model-resolver-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let project = root.join("project");
        let library = root.join("library");
        let outside = root.join("outside");
        for directory in [&project, &library, &outside] {
            fs::create_dir_all(directory).unwrap();
        }
        Self {
            root,
            project,
            library,
            outside,
        }
    }
    fn config(&self) -> ModelResolverConfig {
        let mut config = ModelResolverConfig::new(&self.project);
        config.library_roots.push(self.library.clone());
        config
    }
    fn asset(&self, relative: &str, content: &[u8]) {
        let path = self.project.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
fn model(path: &str) -> ModelReference {
    ModelReference {
        source: SourceLocator {
            kind: "model".into(),
            ordinal: 7,
            component_ordinal: Some(3),
            source_uuid: Some(Arc::from("model-uuid")),
            source_path: vec![3, 7],
        },
        path: Some(path.into()),
        offset: None,
        legacy_at: None,
        rotation_deg: None,
        scale: None,
        hidden: false,
    }
}
#[test]
fn resolves_exact_relative_builtin_library_variable_and_approved_absolute() {
    let fixture = Fixture::new();
    fixture.asset("models/unit.STP", b"synthetic STEP bytes");
    fs::write(fixture.library.join("part.wrl"), b"synthetic WRL bytes").unwrap();
    let mut config = fixture.config();
    config
        .directory_variables
        .insert("KICAD10_3DMODEL_DIR".into(), fixture.library.clone());
    let resolver = ModelResolver::new(config).unwrap();
    for path in [
        "models/unit.STP".to_owned(),
        "${KIPRJMOD}/models/unit.STP".into(),
        fixture
            .project
            .join("models/unit.STP")
            .to_string_lossy()
            .into_owned(),
    ] {
        let resolved = resolver.resolve(&model(&path));
        assert_eq!(resolved.status, Status::Resolved);
        let asset = resolved.asset.unwrap();
        assert_eq!(asset.format(), ModelAssetFormat::Step);
        assert_eq!(asset.bytes(), b"synthetic STEP bytes");
        assert_eq!(asset.sha256().len(), 64);
    }
    for path in ["part.wrl", "${KICAD10_3DMODEL_DIR}/part.wrl"] {
        assert_eq!(resolver.resolve(&model(path)).status, Status::Resolved);
    }
    fs::write(
        fixture.library.join("variant.vrml"),
        b"synthetic VRML bytes",
    )
    .unwrap();
    let variant = resolver.resolve(&model("${KICAD10_3DMODEL_DIR}/variant.vrml"));
    assert_eq!(variant.status, Status::Resolved);
    let variant_asset = variant.asset.unwrap();
    assert_eq!(variant_asset.format(), ModelAssetFormat::Wrl);
    assert_eq!(variant_asset.bytes(), b"synthetic VRML bytes");

    fs::write(fixture.library.join("uppercase.VRML"), b"uppercase alias").unwrap();
    let uppercase = resolver.resolve(&model("${KICAD10_3DMODEL_DIR}/uppercase.VRML"));
    assert_eq!(uppercase.status, Status::Resolved);
    assert_eq!(uppercase.asset.unwrap().format(), ModelAssetFormat::Wrl);
}
#[test]
fn distinguishes_missing_hidden_unsupported_unknown_and_invalid() {
    let fixture = Fixture::new();
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    assert_eq!(
        resolver.resolve(&model("not-present.glb")).status,
        Status::Missing
    );
    assert_eq!(
        resolver.resolve(&model("part.pdf")).status,
        Status::UnsupportedFormat
    );
    assert_eq!(
        resolver.resolve(&model("${UNDEFINED}/part.step")).status,
        Status::UnknownVariable
    );
    assert_eq!(
        resolver.resolve(&model("$BAD/part.step")).status,
        Status::InvalidPath
    );
    let mut reference = model("part.pdf");
    reference.hidden = true;
    assert_eq!(resolver.resolve(&reference).status, Status::Hidden);
    reference.hidden = false;
    reference.path = None;
    assert_eq!(resolver.resolve(&reference).status, Status::MissingPath);
    fs::create_dir(fixture.project.join("directory.step")).unwrap();
    assert_eq!(
        resolver.resolve(&model("directory.step")).status,
        Status::InvalidPath
    );
}
#[test]
fn refuses_ambiguous_matches_and_never_uses_basename_fallback() {
    let fixture = Fixture::new();
    fixture.asset("unit.glb", b"one");
    fs::write(fixture.library.join("unit.glb"), b"two").unwrap();
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    assert_eq!(
        resolver.resolve(&model("unit.glb")).status,
        Status::Ambiguous
    );
    assert_eq!(
        resolver.resolve(&model("somewhere/unit.glb")).status,
        Status::Missing
    );
    let mut config = fixture.config();
    config.cache_roots.push(fixture.library.clone());
    fs::remove_file(fixture.project.join("unit.glb")).unwrap();
    assert_eq!(
        ModelResolver::new(config)
            .unwrap()
            .resolve(&model("unit.glb"))
            .status,
        Status::Resolved
    );
}
#[test]
fn rejects_traversal_windows_network_devices_ads_and_unapproved_paths() {
    let fixture = Fixture::new();
    fs::write(fixture.outside.join("secret.step"), b"outside").unwrap();
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    for path in [
        "../outside/secret.step",
        "models/../unit.step",
        "..\\outside\\secret.step",
        "\\\\server\\share\\part.step",
        "//server/share/part.step",
        "\\\\?\\C:\\part.step",
        "\\\\.\\CON",
        "https://host/part.step",
        "C:part.step",
        "C:/part:stream.step",
        "CON.step",
        "models/NUL.step",
        ".. /outside/secret.step",
        "models./part.step",
        "COM¹.step",
        "CONIN$.step",
        "LPT1.step",
        "part.step\0",
    ] {
        assert_eq!(
            resolver.resolve(&model(path)).status,
            Status::InvalidPath,
            "{path:?}"
        );
    }
    assert_eq!(
        resolver
            .resolve(&model(
                &fixture.outside.join("secret.step").to_string_lossy()
            ))
            .status,
        Status::OutsideApprovedRoots
    );
}
#[test]
fn immutable_capture_and_redacted_projection() {
    let fixture = Fixture::new();
    fixture.asset("private-component.glb", b"first snapshot");
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    let resolved = resolver.resolve(&model("private-component.glb"));
    let first = resolved.asset.as_ref().unwrap();
    fs::write(
        fixture.project.join("private-component.glb"),
        b"second snapshot",
    )
    .unwrap();
    assert_eq!(first.bytes(), b"first snapshot");
    assert_ne!(
        first.sha256(),
        resolver
            .resolve(&model("private-component.glb"))
            .asset
            .unwrap()
            .sha256()
    );
    let json = resolved.shared_projection();
    assert_eq!(json["source"]["source_uuid"], "model-uuid");
    assert_eq!(json["source"]["component_ordinal"], 3);
    assert_eq!(json["source"]["source_path"], serde_json::json!([3, 7]));
    assert_eq!(json["byte_length"], 14);
    let text = json.to_string();
    assert!(!text.contains("private-component"));
    assert!(!text.contains("project"));
    assert!(!text.contains(&fixture.root.to_string_lossy().to_string()));
    let missing = resolver
        .resolve(&model("private-secret-name.step"))
        .shared_projection()
        .to_string();
    assert!(!missing.contains("private-secret-name"));
}
#[test]
fn bounds_file_size_paths_and_configuration() {
    let fixture = Fixture::new();
    fixture.asset("big.step", b"12345");
    let mut config = fixture.config();
    config.max_file_bytes = 4;
    let resolver = ModelResolver::new(config).unwrap();
    assert_eq!(
        resolver.resolve(&model("big.step")).status,
        Status::TooLarge
    );
    assert_eq!(
        resolver
            .resolve(&model(&("a".repeat(4097) + ".step")))
            .status,
        Status::InvalidPath
    );
    let mut config = fixture.config();
    config.max_file_bytes = 0;
    assert!(ModelResolver::new(config).is_err());
    let mut config = fixture.config();
    config.library_roots = vec![fixture.library.clone(); 64];
    assert!(ModelResolver::new(config).is_err());
    let mut config = fixture.config();
    config.directory_variables = BTreeMap::from([("KIPRJMOD".into(), fixture.project.clone())]);
    assert!(ModelResolver::new(config).is_err());
    let mut config = fixture.config();
    config
        .directory_variables
        .insert("BAD".into(), fixture.outside.clone());
    assert!(ModelResolver::new(config).is_err());
    let mut config = fixture.config();
    config.project_root = PathBuf::from("relative");
    assert!(ModelResolver::new(config).is_err());
    let mut config = fixture.config();
    config.project_root = fixture.project.join("missing");
    let error = ModelResolver::new(config).unwrap_err().to_string();
    assert!(!error.contains(&fixture.project.to_string_lossy().to_string()));
}
#[cfg(unix)]
#[test]
fn rejects_symlink_files_and_ancestor_roots() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fs::write(fixture.outside.join("real.step"), b"bytes").unwrap();
    symlink(
        fixture.outside.join("real.step"),
        fixture.project.join("linked.step"),
    )
    .unwrap();
    symlink(&fixture.outside, fixture.project.join("linked-dir")).unwrap();
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    assert_eq!(
        resolver.resolve(&model("linked.step")).status,
        Status::InvalidPath
    );
    assert_eq!(
        resolver.resolve(&model("linked-dir/real.step")).status,
        Status::InvalidPath
    );
    let mut config = fixture.config();
    config
        .library_roots
        .push(fixture.project.join("linked-dir"));
    assert!(ModelResolver::new(config).is_err());
}
#[cfg(windows)]
#[test]
fn rejects_windows_directory_junctions_without_symlink_privilege() {
    use std::os::windows::process::CommandExt;
    let fixture = Fixture::new();
    fs::write(fixture.outside.join("real.step"), b"bytes").unwrap();
    let link = fixture.project.join("linked-dir");
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        // CARGO_MANIFEST_DIR may use forward slashes; cmd's mklink treats
        // those path segments as switches. Preserve Unicode while using
        // native Windows separators for this fixture-only shell boundary.
        .arg(link.to_str().unwrap().replace('/', "\\"))
        .arg(fixture.outside.to_str().unwrap().replace('/', "\\"))
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "junction setup failed: {} / {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    assert_eq!(
        resolver.resolve(&model("linked-dir/real.step")).status,
        Status::InvalidPath
    );
    let mut config = fixture.config();
    config.library_roots.push(link.clone());
    assert!(ModelResolver::new(config).is_err());
    fs::remove_dir(link).unwrap();
}

#[test]
fn differently_cased_absolute_root_is_rejected_conservatively() {
    let fixture = Fixture::new();
    fixture.asset("unit.step", b"synthetic asset");
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    let alias = fixture.root.join("PROJECT/unit.step");
    assert_eq!(
        resolver.resolve(&model(&alias.to_string_lossy())).status,
        Status::OutsideApprovedRoots
    );
    assert_eq!(
        resolver.resolve(&model("unit.step")).status,
        Status::Resolved
    );
}

#[test]
fn malformed_variable_identifiers_are_invalid_paths() {
    let fixture = Fixture::new();
    let resolver = ModelResolver::new(fixture.config()).unwrap();
    for path in [
        "${}/unit.step".to_owned(),
        "${BAD-NAME}/unit.step".to_owned(),
        format!("${{{}}}/unit.step", "A".repeat(129)),
    ] {
        assert_eq!(resolver.resolve(&model(&path)).status, Status::InvalidPath);
    }
    assert_eq!(
        resolver.resolve(&model("${VALID_NAME}/unit.step")).status,
        Status::UnknownVariable
    );
}
