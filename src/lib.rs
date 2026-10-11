//! Pure Rust, offline PCB import and a partial canonical physical document.
//! CLI parsing is separate; existing v0.1 inspection/package projections remain stable.
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub mod assigned_assembly;
pub mod bom;
mod cli;
pub mod component_forge;
pub mod component_pose;
pub mod dataset;
pub mod dataset_external;
pub mod dataset_git;
pub mod digital_twin;
pub mod document;
pub mod footprint_family;
mod geometry;
mod glb;
pub mod model_assembly;
pub mod model_assignments;
pub mod model_mesh;
pub mod model_resolver;
mod package_input;
pub mod population_audit;
pub mod pos_export;
pub mod project_bundle;
pub mod project_intake;
pub mod source_candidate;
mod strict_json;
pub mod topology;
pub mod twin_viewer;

const MAX_INPUT_BYTES: usize = 256 * 1024 * 1024;
const MIN_REVIEWED_BOARD_FORMAT_VERSION: u64 = 20211014;
const MAX_REVIEWED_BOARD_FORMAT_VERSION: u64 = 20260206;
const MAX_TOKENS: usize = 5_000_000;
const MAX_NODES: usize = 2_000_000;
const MAX_DEPTH: usize = 512;

const SHA256_ROUND_CONSTANTS: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

#[derive(Debug)]
pub struct EngineError(String);

type CliError = EngineError;

impl EngineError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

#[derive(Debug, Clone)]
enum Token {
    Open,
    Close,
    Value(String),
}

#[derive(Debug)]
pub enum SourceNode {
    Atom(String),
    List(Vec<Sexp>),
}

type Sexp = SourceNode;

impl SourceNode {
    fn items(&self) -> Option<&[Sexp]> {
        match self {
            Self::List(items) => Some(items),
            Self::Atom(_) => None,
        }
    }

    fn atom(&self) -> Option<&str> {
        match self {
            Self::Atom(value) => Some(value),
            Self::List(_) => None,
        }
    }

    fn head(&self) -> Option<&str> {
        self.items()?.first()?.atom()
    }

    fn child(&self, name: &str) -> Option<&Sexp> {
        self.items()?
            .iter()
            .skip(1)
            .find(|item| item.head() == Some(name))
    }

    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.items()
            .into_iter()
            .flat_map(|items| items.iter().skip(1))
            .filter(move |item| item.head() == Some(name))
    }

    fn value_at(&self, index: usize) -> Option<&str> {
        self.items()?.get(index)?.atom()
    }
}

#[derive(Debug)]
struct Footprint {
    reference: String,
    reference_is_placeholder: bool,
    value: String,
    library_id: String,
    side: String,
    x_mm: Option<f64>,
    y_mm: Option<f64>,
    rotation_deg: Option<f64>,
    uuid: String,
    pad_count: usize,
    model_count: usize,
    dnp: bool,
    excluded_from_pos_files: bool,
}

#[derive(Debug)]
pub struct PhysicalBoardDocument {
    source: PathBuf,
    source_bytes: usize,
    source_sha256: String,
    version: u64,
    thickness_mm: Option<f64>,
    layers: Vec<String>,
    footprints: Vec<Footprint>,
    net_count: usize,
    track_segment_count: usize,
    track_arc_count: usize,
    via_count: usize,
    zone_count: usize,
    edge_cuts_count: usize,
    diagnostics: Vec<String>,
    geometry: geometry::Geometry,
    outline: topology::OutlineReport,
    engineering: document::EngineeringData,
    source_document: Sexp,
}

type Board = PhysicalBoardDocument;

/// Supported display sides. Bottom mirroring affects the SVG view only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgSide {
    Top,
    Bottom,
}

/// Controls whether an explicitly incomplete GLB may be produced.
/// The current board-shell-only exporter is always partial and is refused by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GlbExportOptions {
    pub allow_partial: bool,
}

/// Options for source-model assembly exports. The filter is opt-in and applies
/// only to canonical source footprints carrying a native `(attr dnp)` value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GlbModelExportOptions {
    pub allow_partial: bool,
    pub exclude_dnp: bool,
}

/// Controls the supported SVG projection without changing the historical default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SvgExportOptions {
    /// Omit text and graphic primitives owned by DNP footprints on `F.Fab`/`B.Fab`.
    /// Pads and geometry on every other layer remain visible.
    pub hide_dnp_footprints_on_fab_layers: bool,
}

pub use glb::{
    AssignedGlbExport, DnpFilterCoverage, GlbCoverageReport, GlbExport, GlbModelExport,
    UnresolvedComponentCoverage,
};

impl SvgSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}
impl TryFrom<&str> for SvgSide {
    type Error = EngineError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            _ => Err(EngineError::new("SVG side must be 'top' or 'bottom'")),
        }
    }
}
/// Coordinate declaration for imported physical source data, not a display mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalUnits {
    Millimetres,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCoordinateFrame {
    KiCadBoardYDown,
}
#[derive(Debug, Clone, Copy)]
pub struct SourceMetadata<'a> {
    pub path: &'a Path,
    pub bytes: usize,
    pub sha256: &'a str,
    pub format_version: u64,
    pub units: PhysicalUnits,
    pub coordinate_frame: SourceCoordinateFrame,
}
impl std::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for EngineError {}

impl PhysicalBoardDocument {
    pub fn source(&self) -> SourceMetadata<'_> {
        SourceMetadata {
            path: &self.source,
            bytes: self.source_bytes,
            sha256: &self.source_sha256,
            format_version: self.version,
            units: PhysicalUnits::Millimetres,
            coordinate_frame: SourceCoordinateFrame::KiCadBoardYDown,
        }
    }
    pub fn thickness_mm(&self) -> Option<f64> {
        self.thickness_mm
    }
    /// Source-semantic records retain unsupported/malformed data and diagnostics.
    pub fn engineering(&self) -> &document::EngineeringData {
        &self.engineering
    }
    /// Supported board-space render projection, derived once during import.
    /// Pad/text angles here use SVG display sign; engineering records keep source angles.
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }
    /// Physical centerline contours and topology validation, separate from the
    /// frozen v0.1 drawing bounds/diagnostics and overall CAD coverage.
    pub fn outline(&self) -> &topology::OutlineReport {
        &self.outline
    }
    /// Opt into bounded endpoint chaining without editing saved source geometry.
    /// Unsupported diagnostics cannot be bypassed by this policy.
    pub fn with_outline_chaining_tolerance(
        mut self,
        tolerance_mm: f64,
    ) -> Result<Self, EngineError> {
        if ![topology::JOIN_TOLERANCE_MM, 0.001, 0.01].contains(&tolerance_mm) {
            return Err(EngineError::new(
                "export chaining tolerance must be 0.000001, 0.001 or 0.01 mm",
            ));
        }
        if self
            .outline
            .diagnostics
            .iter()
            .any(|d| d.code != topology::OutlineDiagnosticCode::OpenContour)
        {
            return Err(EngineError::new(
                "chaining cannot bypass non-endpoint outline diagnostics",
            ));
        }
        self.outline = topology::from_engineering_with_tolerance(&self.engineering, tolerance_mm);
        Ok(self)
    }
    /// Separate physical-outline report; the v0.1 inspection schema is unchanged.
    pub fn outline_json(&self) -> String {
        use serde_json::json;
        let point = |p: document::Point2| json!([p.x, p.y]);
        let source = |s: &document::SourceLocator| {
            json!({
                "kind": s.kind, "path": s.source_path,
                "uuid": s.source_uuid.as_deref(), "componentOrdinal": s.component_ordinal
            })
        };
        let primitives = self.outline.primitives.iter().map(|p| {
            let shape = match p.shape {
                topology::OutlineShape::Line { start, end } => json!({"kind":"line", "start":point(start), "end":point(end)}),
                topology::OutlineShape::Arc { start, mid, end } => {
                    let circle = topology::arc_parameters(start, mid, end).map(|(center,radius,_,_,_)|
                        json!({"center":point(center),"radiusMm":radius}));
                    json!({"kind":"arc", "start":point(start), "mid":point(mid), "end":point(end), "circle":circle})
                },
                topology::OutlineShape::Circle { center, radius_mm } => json!({"kind":"circle", "center":point(center), "radiusMm":radius_mm}),
            };
            json!({"source":source(&p.source), "shape":shape})
        }).collect::<Vec<_>>();
        let contours = self.outline.contours.iter().map(|c| json!({
            "role":match c.role { topology::ContourRole::Outer => "outer", topology::ContourRole::Cutout => "cutout" },
            "signedAreaMm2":c.signed_area_mm2,
            "edges":c.edges.iter().map(|e| json!({"primitive":e.primitive_index, "reversed":e.reversed})).collect::<Vec<_>>()
        })).collect::<Vec<_>>();
        let diagnostics = self
            .outline
            .diagnostics
            .iter()
            .map(|d| {
                json!({
                    "code":format!("{:?}",d.code), "message":d.message,
                    "source":d.source.as_ref().map(source)
                })
            })
            .collect::<Vec<_>>();
        let bounds = self
            .outline
            .bounds
            .map(|b| json!({"minX":b.min_x,"minY":b.min_y,"maxX":b.max_x,"maxY":b.max_y}));
        let report = json!({
            "reportVersion":"1.0.0", "sourceSha256":self.source_sha256,
            "units":"mm", "coordinateFrame":"KiCad-board-Y-down",
            "valid":self.outline.is_valid(), "joinToleranceMm":self.outline.join_tolerance_mm,
            "bounds":bounds, "primitives":primitives, "contours":contours,
            "diagnostics":diagnostics, "geometryComplete":false,
            "scope":"Supported Edge.Cuts topology only; not DRC or complete CAD validation"
        });
        format!(
            "{}\n",
            serde_json::to_string_pretty(&report).expect("finite physical report")
        )
    }
    /// Parsed source syntax is retained for fields not interpreted by this subset.
    pub fn source_syntax(&self) -> &SourceNode {
        &self.source_document
    }
    /// Recover the saved source node addressed by a canonical record/diagnostic.
    /// Paths use child ordinals excluding each list's head atom.
    pub fn source_node(&self, locator: &document::SourceLocator) -> Option<&SourceNode> {
        source_at_path(&self.source_document, &locator.source_path)
    }
    /// The engine remains a partial importer; no full topology/CAD conformance claim.
    pub fn geometry_complete(&self) -> bool {
        false
    }
    pub fn inspection_json(&self) -> String {
        inspect_json(self)
    }
    pub fn placements_csv(&self) -> Result<String, EngineError> {
        placements_csv(self)
    }
    pub fn svg(&self, side: SvgSide, layers: Option<&[String]>) -> Result<SvgOutput, EngineError> {
        self.svg_with_options(side, layers, SvgExportOptions::default())
    }
    pub fn svg_with_options(
        &self,
        side: SvgSide,
        layers: Option<&[String]>,
        options: SvgExportOptions,
    ) -> Result<SvgOutput, EngineError> {
        if !self.outline.is_valid() {
            let reason = self.outline.diagnostics.first().map_or_else(
                || "no validated physical board contour".to_owned(),
                |diagnostic| format!("{:?}: {}", diagnostic.code, diagnostic.message),
            );
            return Err(EngineError::new(format!(
                "SVG export requires valid physical outline topology: {reason}"
            )));
        }
        let dnp_component_ids = if options.hide_dnp_footprints_on_fab_layers {
            let mut counts = std::collections::BTreeMap::<&str, usize>::new();
            for footprint in &self.footprints {
                if !footprint.uuid.is_empty() {
                    *counts.entry(&footprint.uuid).or_default() += 1;
                }
            }
            let mut dnp_component_ids = BTreeSet::new();
            for footprint in self.footprints.iter().filter(|footprint| footprint.dnp) {
                if footprint.uuid.is_empty() {
                    return Err(EngineError::new(
                        "cannot hide DNP Fab graphics because a DNP footprint has no source UUID",
                    ));
                }
                let source_component = self
                    .engineering
                    .components
                    .iter()
                    .find(|component| {
                        component.source.kind == "footprint"
                            && component.source.source_uuid.as_deref()
                                == Some(footprint.uuid.as_str())
                    })
                    .and_then(|component| {
                        source_at_path(
                            &self.source_document,
                            &component.source.source_path,
                        )
                    })
                    .ok_or_else(|| {
                        EngineError::new(
                            "cannot hide DNP Fab graphics because its source footprint is ambiguous",
                        )
                    })?;
                let source_uuids: Vec<_> = source_component.children("uuid").collect();
                match source_uuids.len() {
                    0 => {
                        return Err(EngineError::new(
                            "cannot hide DNP Fab graphics because a DNP footprint has no source UUID",
                        ));
                    }
                    1 => {}
                    _ => {
                        return Err(EngineError::new(
                            "cannot hide DNP Fab graphics because a DNP footprint has multiple source UUID fields",
                        ));
                    }
                }
                if source_uuids[0].items().is_none_or(|items| items.len() != 2) {
                    return Err(EngineError::new(
                        "cannot hide DNP Fab graphics because a DNP footprint has a malformed source UUID field",
                    ));
                }
                if source_uuids[0].value_at(1) != Some(footprint.uuid.as_str()) {
                    return Err(EngineError::new(
                        "cannot hide DNP Fab graphics because its source UUID is missing or ambiguous",
                    ));
                }
                if counts.get(footprint.uuid.as_str()) != Some(&1) {
                    return Err(EngineError::new(
                        "cannot hide DNP Fab graphics because a DNP footprint UUID is ambiguous",
                    ));
                }
                dnp_component_ids.insert(footprint.uuid.clone());
            }
            Some(dnp_component_ids)
        } else {
            None
        };
        geometry::render_validated_svg(
            &self.geometry,
            &self.layers,
            side.as_str(),
            layers,
            dnp_component_ids.as_ref(),
        )
    }
    /// Export a self-contained GLB board shell from validated physical topology.
    /// This initial GLB slice does not resolve component models; set
    /// `allow_partial` explicitly to export a shell while components are missing.
    pub fn export_glb(&self, options: GlbExportOptions) -> Result<GlbExport, EngineError> {
        glb::export_board_shell(self, options.allow_partial)
    }

    /// Export explicitly assigned bodies; authored PCB model records are unchanged.
    pub fn export_glb_with_assignments(
        &self,
        assignments: &model_assignments::ValidatedModelAssignments,
        options: GlbExportOptions,
    ) -> Result<AssignedGlbExport, EngineError> {
        glb::export_assigned_board_assembly(self, assignments, options.allow_partial)
    }

    fn dnp_component_ordinals(&self) -> Result<std::collections::BTreeSet<usize>, EngineError> {
        let mut by_uuid = std::collections::BTreeMap::<&str, Vec<usize>>::new();
        for component in &self.engineering().components {
            if let Some(uuid) = component
                .source
                .source_uuid
                .as_deref()
                .filter(|uuid| !uuid.is_empty())
            {
                by_uuid
                    .entry(uuid)
                    .or_default()
                    .push(component.source.ordinal);
            }
        }
        let mut excluded = std::collections::BTreeSet::new();
        for component in &self.engineering().components {
            let source = source_at_path(&self.source_document, &component.source.source_path)
                .ok_or_else(|| {
                    EngineError::new("cannot safely apply --no-dnp: missing source component")
                })?;
            if !has_native_dnp_attribute(source) {
                continue;
            }

            let fields: Vec<_> = source.children("uuid").collect();
            if fields.len() > 1 {
                return Err(EngineError::new(
                    "cannot safely apply --no-dnp: malformed or multiple source UUID fields",
                ));
            }
            let field = fields.first().copied().ok_or_else(|| {
                EngineError::new(
                    "cannot safely apply --no-dnp: a DNP footprint has a missing or ambiguous canonical UUID",
                )
            })?;
            let values = field.items().ok_or_else(|| {
                EngineError::new(
                    "cannot safely apply --no-dnp: malformed or multiple source UUID fields",
                )
            })?;
            if values.len() > 2 {
                return Err(EngineError::new(
                    "cannot safely apply --no-dnp: malformed or multiple source UUID fields",
                ));
            }
            let uuid = field
                .value_at(1)
                .filter(|uuid| !uuid.is_empty())
                .ok_or_else(|| {
                    EngineError::new(
                        "cannot safely apply --no-dnp: a DNP footprint has a missing or ambiguous canonical UUID",
                    )
                })?;
            if component.source.source_uuid.as_deref() != Some(uuid) {
                return Err(EngineError::new(
                    "cannot safely apply --no-dnp: malformed or multiple source UUID fields",
                ));
            }

            let ordinals = by_uuid
                .get(uuid)
                .filter(|ordinals| ordinals.len() == 1)
                .ok_or_else(|| {
                    EngineError::new(
                        "cannot safely apply --no-dnp: a DNP footprint has a missing or ambiguous canonical UUID",
                    )
                })?;
            if ordinals[0] != component.source.ordinal {
                return Err(EngineError::new(
                    "cannot safely apply --no-dnp: a DNP footprint has a missing or ambiguous canonical UUID",
                ));
            }
            excluded.insert(ordinals[0]);
        }
        Ok(excluded)
    }

    /// Export the board shell plus the strictly supported locally resolved VRML
    /// model subset. STEP, GLB and glTF source models are retained as explicit
    /// unresolved coverage; they are never approximated or converted here.
    pub fn export_glb_with_models(
        &self,
        resolver: &model_resolver::ModelResolver,
        options: GlbExportOptions,
    ) -> Result<GlbExport, EngineError> {
        let export = self.export_glb_with_model_options(
            resolver,
            GlbModelExportOptions {
                allow_partial: options.allow_partial,
                exclude_dnp: false,
            },
        )?;
        Ok(GlbExport {
            bytes: export.bytes,
            report: export.report,
        })
    }

    /// Export the supported local VRML subset with explicit model filters.
    /// The legacy `export_glb_with_models` method preserves the unfiltered default.
    pub fn export_glb_with_model_options(
        &self,
        resolver: &model_resolver::ModelResolver,
        options: GlbModelExportOptions,
    ) -> Result<GlbModelExport, EngineError> {
        glb::export_board_assembly(self, resolver, options.allow_partial, options.exclude_dnp)
    }
    pub fn build_package(&self, output: impl AsRef<Path>) -> Result<(), EngineError> {
        package_build(self, output.as_ref())
    }
}
/// Import a supported KiCad physical PCB without invoking KiCad or Cloud services.
pub fn load_kicad_board(path: impl AsRef<Path>) -> Result<PhysicalBoardDocument, EngineError> {
    load_board(path.as_ref())
}
/// Import a KiCad PCB or verify/reimport a fixed-layout v0.1 Tokito package.
pub fn load_board_input(path: impl AsRef<Path>) -> Result<PhysicalBoardDocument, EngineError> {
    package_input::load_input(path.as_ref())
}
pub fn verify_package(path: impl AsRef<Path>) -> Result<PhysicalBoardDocument, EngineError> {
    package_input::load_package(path.as_ref())
}

pub use geometry::{Bounds, Drill, Geometry, Item, Point, Shape, SvgOutput};
/// Binary entry point; library consumers should use the typed engine operations above.
#[doc(hidden)]
pub fn run_cli(args: &[String]) -> Result<(), EngineError> {
    cli::run(args)
}

fn lex(input: &[u8]) -> Result<Vec<Token>, CliError> {
    let mut tokens = Vec::new();
    let mut cursor = if input.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };

    while cursor < input.len() {
        match input[cursor] {
            b' ' | b'\t' | b'\r' | b'\n' => cursor += 1,
            b';' => {
                while cursor < input.len() && input[cursor] != b'\n' {
                    cursor += 1;
                }
            }
            b'(' => {
                tokens.push(Token::Open);
                cursor += 1;
            }
            b')' => {
                tokens.push(Token::Close);
                cursor += 1;
            }
            b'"' => tokens.push(Token::Value(read_quoted(input, &mut cursor)?)),
            _ => {
                let start = cursor;
                while cursor < input.len()
                    && !matches!(
                        input[cursor],
                        b' ' | b'\t' | b'\r' | b'\n' | b'(' | b')' | b';'
                    )
                {
                    cursor += 1;
                }
                if start == cursor {
                    return Err(CliError::new("invalid token at input boundary"));
                }
                let value = std::str::from_utf8(&input[start..cursor])
                    .map_err(|_| CliError::new("board file contains invalid UTF-8"))?;
                tokens.push(Token::Value(value.to_owned()));
            }
        }
        if tokens.len() > MAX_TOKENS {
            return Err(CliError::new(format!(
                "board file exceeds the token limit ({MAX_TOKENS})"
            )));
        }
    }
    Ok(tokens)
}

fn read_quoted(input: &[u8], cursor: &mut usize) -> Result<String, CliError> {
    *cursor += 1;
    let mut output = Vec::new();
    while *cursor < input.len() {
        match input[*cursor] {
            b'"' => {
                *cursor += 1;
                return String::from_utf8(output)
                    .map_err(|_| CliError::new("quoted value decodes to invalid UTF-8"));
            }
            b'\\' => {
                *cursor += 1;
                let escaped = *input
                    .get(*cursor)
                    .ok_or_else(|| CliError::new("unterminated escape in quoted value"))?;
                *cursor += 1;
                match escaped {
                    b'n' => output.push(b'\n'),
                    b'r' => output.push(b'\r'),
                    b't' => output.push(b'\t'),
                    b'"' => output.push(b'"'),
                    b'\\' => output.push(b'\\'),
                    b'x' => {
                        let high = input
                            .get(*cursor)
                            .copied()
                            .and_then(hex_value)
                            .ok_or_else(|| CliError::new("invalid hexadecimal escape"))?;
                        let low = input
                            .get(*cursor + 1)
                            .copied()
                            .and_then(hex_value)
                            .ok_or_else(|| CliError::new("invalid hexadecimal escape"))?;
                        output.push((high << 4) | low);
                        *cursor += 2;
                    }
                    b'0'..=b'7' => {
                        let first = escaped - b'0';
                        let second = input
                            .get(*cursor)
                            .copied()
                            .filter(|value| (b'0'..=b'7').contains(value))
                            .ok_or_else(|| CliError::new("invalid octal escape"))?
                            - b'0';
                        let third = input
                            .get(*cursor + 1)
                            .copied()
                            .filter(|value| (b'0'..=b'7').contains(value))
                            .ok_or_else(|| CliError::new("invalid octal escape"))?
                            - b'0';
                        output.push((first << 6) | (second << 3) | third);
                        *cursor += 2;
                    }
                    _ => return Err(CliError::new("unsupported escape in quoted value")),
                }
            }
            byte => {
                output.push(byte);
                *cursor += 1;
            }
        }
    }
    Err(CliError::new("unterminated quoted value"))
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn parse_one(
    tokens: &[Token],
    cursor: &mut usize,
    depth: usize,
    nodes: &mut usize,
) -> Result<Sexp, CliError> {
    if depth > MAX_DEPTH {
        return Err(CliError::new(format!(
            "S-expression nesting exceeds the depth limit ({MAX_DEPTH})"
        )));
    }
    *nodes += 1;
    if *nodes > MAX_NODES {
        return Err(CliError::new(format!(
            "board file exceeds the node limit ({MAX_NODES})"
        )));
    }
    let token = tokens
        .get(*cursor)
        .ok_or_else(|| CliError::new("unexpected end of board file"))?;
    *cursor += 1;
    match token {
        Token::Value(value) => Ok(Sexp::Atom(value.clone())),
        Token::Close => Err(CliError::new("unexpected closing parenthesis")),
        Token::Open => {
            let mut items = Vec::new();
            loop {
                match tokens.get(*cursor) {
                    Some(Token::Close) => {
                        *cursor += 1;
                        return Ok(Sexp::List(items));
                    }
                    None => return Err(CliError::new("unclosed list in board file")),
                    _ => items.push(parse_one(tokens, cursor, depth + 1, nodes)?),
                }
            }
        }
    }
}

fn parse_document(input: &[u8]) -> Result<Sexp, CliError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(CliError::new(format!(
            "board file exceeds the size limit ({} bytes)",
            MAX_INPUT_BYTES
        )));
    }
    let tokens = lex(input)?;
    let mut cursor = 0;
    let mut nodes = 0;
    let root = parse_one(&tokens, &mut cursor, 0, &mut nodes)?;
    if cursor != tokens.len() {
        return Err(CliError::new("unexpected content after board document"));
    }
    Ok(root)
}

fn parse_number(value: Option<&str>) -> Option<f64> {
    let number = value?.parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn child_number(node: &Sexp, name: &str, index: usize) -> Option<f64> {
    parse_number(node.child(name)?.value_at(index))
}

fn source_at_path<'a>(root: &'a Sexp, path: &[usize]) -> Option<&'a Sexp> {
    let mut node = root;
    for &ordinal in path {
        node = node.items()?.get(ordinal.checked_add(1)?)?;
    }
    Some(node)
}

#[cfg(test)]
fn property_value(node: &Sexp, property_name: &str) -> Option<String> {
    for property in node.children("property") {
        if property.value_at(1) == Some(property_name) {
            return property.value_at(2).map(str::to_owned);
        }
    }
    for text in node.children("fp_text") {
        if text
            .value_at(1)
            .is_some_and(|kind| kind.eq_ignore_ascii_case(property_name))
        {
            return text.value_at(2).map(str::to_owned);
        }
    }
    None
}

fn is_placeholder_reference(reference: &str) -> bool {
    let reference = reference.trim();
    reference.is_empty() || matches!(reference, "G***" | "${REFERENCE}")
}

fn is_dnp(node: &Sexp) -> bool {
    has_native_dnp_attribute(node)
        || node.children("property").any(|property| {
            property
                .value_at(1)
                .is_some_and(|name| name.eq_ignore_ascii_case("DNP"))
                && property
                    .value_at(2)
                    .is_some_and(|value| value.eq_ignore_ascii_case("yes"))
        })
}

fn has_native_dnp_attribute(node: &Sexp) -> bool {
    has_attribute(node, "dnp")
}

fn has_attribute(node: &Sexp, expected: &str) -> bool {
    node.children("attr").any(|attribute| {
        attribute
            .items()
            .unwrap_or_default()
            .iter()
            .skip(1)
            .filter_map(Sexp::atom)
            .any(|value| value.eq_ignore_ascii_case(expected))
    })
}

#[cfg(test)]
fn read_footprint(node: &Sexp) -> Footprint {
    let library_id = node.value_at(1).unwrap_or_default().to_owned();
    let layer = node
        .child("layer")
        .and_then(|child| child.value_at(1))
        .unwrap_or("unknown");
    let (x_mm, y_mm, rotation_deg) = match node.child("at") {
        Some(at) => {
            let rotation = if at.items().is_some_and(|items| items.len() > 3) {
                parse_number(at.value_at(3))
            } else {
                Some(0.0)
            };
            (
                parse_number(at.value_at(1)),
                parse_number(at.value_at(2)),
                rotation,
            )
        }
        None => (None, None, Some(0.0)),
    };
    let uuid = node
        .child("uuid")
        .or_else(|| node.child("tstamp"))
        .and_then(|child| child.value_at(1))
        .unwrap_or_default()
        .to_owned();
    let reference = property_value(node, "Reference").unwrap_or_default();
    let reference_is_placeholder = is_placeholder_reference(&reference);
    Footprint {
        reference,
        reference_is_placeholder,
        value: property_value(node, "Value").unwrap_or_default(),
        library_id,
        side: match layer {
            "F.Cu" => "front",
            "B.Cu" => "back",
            _ => "unknown",
        }
        .to_owned(),
        x_mm,
        y_mm,
        rotation_deg,
        uuid,
        pad_count: node.children("pad").count(),
        model_count: node.children("model").count(),
        dnp: is_dnp(node),
        excluded_from_pos_files: has_attribute(node, "exclude_from_pos_files"),
    }
}

/// Frozen v0.1 inventory projection from canonical component records.
/// Raw flags and independently malformed XY scalars retain legacy behavior;
/// valid identities and poses have one authoritative typed interpretation.
fn project_footprints(document: &Sexp, data: &document::EngineeringData) -> Vec<Footprint> {
    let mut counts = std::collections::BTreeMap::<usize, (usize, usize)>::new();
    for pad in &data.pads {
        if let Some(parent) = pad.source.component_ordinal {
            counts.entry(parent).or_default().0 += 1;
        }
    }
    for model in &data.models {
        if let Some(parent) = model.source.component_ordinal {
            counts.entry(parent).or_default().1 += 1;
        }
    }
    data.components
        .iter()
        .filter(|component| component.source.kind == "footprint")
        .filter_map(|component| {
            let node = source_at_path(document, &component.source.source_path)?;
            let pose = component.source_board_pose;
            let reference = component.reference.clone().unwrap_or_default();
            let (pad_count, model_count) = counts
                .get(&component.source.ordinal)
                .copied()
                .unwrap_or_default();
            Some(Footprint {
                reference_is_placeholder: is_placeholder_reference(&reference),
                reference,
                value: component.value.clone().unwrap_or_default(),
                library_id: component.library_id.clone().unwrap_or_default(),
                side: match component.layer.as_deref() {
                    Some("F.Cu") => "front",
                    Some("B.Cu") => "back",
                    _ => "unknown",
                }
                .to_owned(),
                x_mm: pose
                    .position_mm
                    .map(|point| point.x)
                    .or_else(|| child_number(node, "at", 1)),
                y_mm: pose
                    .position_mm
                    .map(|point| point.y)
                    .or_else(|| child_number(node, "at", 2)),
                rotation_deg: pose.effective_rotation_deg(),
                uuid: component
                    .source
                    .source_uuid
                    .as_deref()
                    .unwrap_or_default()
                    .to_owned(),
                pad_count,
                model_count,
                dnp: is_dnp(node),
                excluded_from_pos_files: has_attribute(node, "exclude_from_pos_files"),
            })
        })
        .collect()
}

fn has_edge_cuts_layer(item: &Sexp) -> bool {
    item.child("layer").and_then(|layer| layer.value_at(1)) == Some("Edge.Cuts")
}

/// Unknown outline source syntax cannot disappear behind a valid rendered
/// subset. This check affects physical operations only, never frozen inventory.
fn reject_uninterpreted_outline(
    source: &Sexp,
    engineering: &document::EngineeringData,
    report: &mut topology::OutlineReport,
) {
    let known = engineering
        .graphics
        .iter()
        .map(|graphic| graphic.source.source_path.as_slice())
        .collect::<BTreeSet<_>>();
    let declares_outline = |node: &Sexp| {
        has_edge_cuts_layer(node)
            || node.child("layers").is_some_and(|layers| {
                layers
                    .items()
                    .unwrap_or_default()
                    .iter()
                    .skip(1)
                    .any(|value| value.atom() == Some("Edge.Cuts"))
            })
    };
    let mut check = |node: &Sexp, path: Vec<usize>, parent: Option<usize>| {
        if declares_outline(node)
            && !known.contains(path.as_slice())
            && report.diagnostics.len() < 200
        {
            report.diagnostics.push(topology::OutlineDiagnostic {
                code: topology::OutlineDiagnosticCode::UnsupportedPrimitive,
                source: Some(document::SourceLocator {
                    kind: node.head().unwrap_or("unknown").to_owned(),
                    ordinal: *path.last().unwrap_or(&0),
                    component_ordinal: parent,
                    source_uuid: node
                        .child("uuid")
                        .or_else(|| node.child("tstamp"))
                        .and_then(|identity| identity.value_at(1))
                        .map(std::sync::Arc::from),
                    source_path: path,
                }),
                message:
                    "Uninterpreted Edge.Cuts source object prevents physical outline certification"
                        .to_owned(),
            });
        }
    };
    for (ordinal, node) in source
        .items()
        .unwrap_or_default()
        .iter()
        .skip(1)
        .enumerate()
    {
        check(node, vec![ordinal], None);
        if matches!(node.head(), Some("footprint" | "module")) {
            for (child_ordinal, child) in
                node.items().unwrap_or_default().iter().skip(1).enumerate()
            {
                check(child, vec![ordinal, child_ordinal], Some(ordinal));
            }
        }
    }
}

fn read_bounded_file(path: &Path, limit: usize) -> Result<Vec<u8>, CliError> {
    let file = fs::File::open(path)
        .map_err(|error| CliError::new(format!("cannot read file {}: {error}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| CliError::new(format!("cannot read file {}: {error}", path.display())))?;
    if bytes.len() > limit {
        return Err(CliError::new(format!(
            "file exceeds the size limit ({limit} bytes): {}",
            path.display()
        )));
    }
    Ok(bytes)
}

fn load_board(path: &Path) -> Result<Board, CliError> {
    if !path.is_file() {
        return Err(CliError::new(format!(
            "input file does not exist: {}",
            path.display()
        )));
    }
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("kicad_pcb"))
    {
        return Err(CliError::new("input must have a .kicad_pcb extension"));
    }
    let bytes = read_bounded_file(path, MAX_INPUT_BYTES)?;
    let document = parse_document(&bytes)?;
    if document.head() != Some("kicad_pcb") {
        return Err(CliError::new(
            "input is not a KiCad PCB S-expression document",
        ));
    }

    let version = document
        .child("version")
        .and_then(|node| node.value_at(1))
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| CliError::new("missing or invalid KiCad PCB format version"))?;
    if !(MIN_REVIEWED_BOARD_FORMAT_VERSION..=MAX_REVIEWED_BOARD_FORMAT_VERSION).contains(&version) {
        return Err(CliError::new(format!(
            "unsupported KiCad PCB format version {version}; this build accepts reviewed format dates {MIN_REVIEWED_BOARD_FORMAT_VERSION} through {MAX_REVIEWED_BOARD_FORMAT_VERSION}"
        )));
    }
    let thickness_mm = document
        .child("general")
        .and_then(|general| child_number(general, "thickness", 1));
    let engineering = document::EngineeringData::from_source(&document);
    let layers = engineering
        .layers
        .iter()
        .filter_map(|layer| layer.name.clone())
        .collect();
    let footprints = project_footprints(&document, &engineering);
    let edge_cuts_count = document
        .items()
        .into_iter()
        .flat_map(|items| items.iter().skip(1))
        .filter(|item| has_edge_cuts_layer(item))
        .count();
    let mut diagnostics = Vec::new();
    if thickness_mm.is_none() {
        diagnostics.push("board thickness is not declared in the expected location".to_owned());
    }
    if edge_cuts_count == 0 {
        diagnostics
            .push("no Edge.Cuts objects were found; board outline is unavailable".to_owned());
    }
    for (index, footprint) in footprints.iter().enumerate() {
        if footprint.x_mm.is_none() || footprint.y_mm.is_none() {
            diagnostics.push(format!(
                "footprint {} has no finite XY placement",
                if footprint.reference.is_empty() {
                    format!("at index {}", index + 1)
                } else {
                    footprint.reference.clone()
                }
            ));
        }
        if footprint.rotation_deg.is_none() {
            diagnostics.push(format!(
                "footprint {} has an invalid or non-finite rotation",
                if footprint.reference.is_empty() {
                    format!("at index {}", index + 1)
                } else {
                    footprint.reference.clone()
                }
            ));
        }
        if footprint.reference.is_empty() {
            diagnostics.push(format!(
                "footprint at index {} has no reference designator",
                index + 1
            ));
        }
        if footprint.side == "unknown" {
            diagnostics.push(format!(
                "footprint {} has an unrecognized copper side",
                if footprint.reference.is_empty() {
                    format!("at index {}", index + 1)
                } else {
                    footprint.reference.clone()
                }
            ));
        }
    }
    let mut references = BTreeSet::new();
    for footprint in &footprints {
        if !footprint.reference_is_placeholder
            && !footprint.reference.is_empty()
            && !references.insert(footprint.reference.as_str())
        {
            diagnostics.push(format!(
                "duplicate reference designator: {}",
                footprint.reference
            ));
        }
    }
    let geometry = geometry::parse_geometry(&document, &footprints, &engineering);
    let mut outline = topology::from_engineering(&engineering);
    reject_uninterpreted_outline(&document, &engineering, &mut outline);
    for graphic in &engineering.graphics {
        if graphic.layer.as_deref() == Some("Edge.Cuts")
            && graphic.source.component_ordinal.is_some_and(|ordinal| {
                engineering
                    .components
                    .iter()
                    .any(|c| c.source.ordinal == ordinal && c.source.kind == "module")
            })
            && outline.diagnostics.len() < 200
        {
            outline.diagnostics.push(topology::OutlineDiagnostic {
                code: topology::OutlineDiagnosticCode::UnsupportedPrimitive,
                source: Some(graphic.source.clone()),
                message: "Legacy module-owned Edge.Cuts are retained but not supported by physical SVG export.".into(),
            });
        }
    }
    outline.discard_invalid_geometry();
    Ok(Board {
        source: path.to_path_buf(),
        source_bytes: bytes.len(),
        source_sha256: sha256_hex(&bytes),
        version,
        thickness_mm,
        layers,
        footprints,
        net_count: engineering.nets.len(),
        track_segment_count: engineering.track_segments.len(),
        track_arc_count: engineering.track_arcs.len(),
        via_count: engineering.vias.len(),
        zone_count: engineering
            .zones
            .iter()
            .filter(|zone| zone.frame == document::CoordinateFrame::Board)
            .count(),
        edge_cuts_count,
        diagnostics,
        geometry,
        outline,
        engineering,
        source_document: document,
    })
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn placements_csv(board: &Board) -> Result<String, CliError> {
    if board
        .footprints
        .iter()
        .any(|footprint| footprint.x_mm.is_none() || footprint.y_mm.is_none())
    {
        return Err(CliError::new(
            "placement export stopped because at least one footprint has invalid XY coordinates",
        ));
    }
    if board
        .footprints
        .iter()
        .any(|footprint| footprint.side == "unknown")
    {
        return Err(CliError::new(
            "placement export stopped because at least one footprint side is unsupported",
        ));
    }
    if board
        .footprints
        .iter()
        .any(|footprint| footprint.rotation_deg.is_none())
    {
        return Err(CliError::new(
            "placement export stopped because at least one footprint has an invalid rotation",
        ));
    }
    let mut uuids = BTreeSet::new();
    if board
        .footprints
        .iter()
        .any(|footprint| footprint.uuid.is_empty())
    {
        return Err(CliError::new(
            "placement export stopped because at least one footprint has no stable UUID",
        ));
    }
    if board
        .footprints
        .iter()
        .any(|footprint| !uuids.insert(footprint.uuid.as_str()))
    {
        return Err(CliError::new(
            "placement export stopped because footprint UUIDs are not unique",
        ));
    }
    let mut references = BTreeSet::new();
    if board.footprints.iter().any(|footprint| {
        !footprint.reference_is_placeholder && !references.insert(footprint.reference.as_str())
    }) {
        return Err(CliError::new(
            "placement export stopped because component reference designators are not unique",
        ));
    }
    let mut output = String::from(
        "reference,reference_is_placeholder,value,library_id,side,x_mm,y_mm,rotation_deg,uuid,pad_count,model_count,dnp,excluded_from_pos_files\n",
    );
    for footprint in &board.footprints {
        let row = [
            csv_field(&footprint.reference),
            footprint.reference_is_placeholder.to_string(),
            csv_field(&footprint.value),
            csv_field(&footprint.library_id),
            csv_field(&footprint.side),
            footprint.x_mm.unwrap_or_default().to_string(),
            footprint.y_mm.unwrap_or_default().to_string(),
            footprint.rotation_deg.unwrap_or_default().to_string(),
            csv_field(&footprint.uuid),
            footprint.pad_count.to_string(),
            footprint.model_count.to_string(),
            footprint.dnp.to_string(),
            footprint.excluded_from_pos_files.to_string(),
        ]
        .join(",");
        output.push_str(&row);
        output.push('\n');
    }
    Ok(output)
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), CliError> {
    if path.exists() {
        return Err(CliError::new(format!(
            "output already exists; choose another path: {}",
            path.display()
        )));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(CliError::new(format!(
            "output directory does not exist: {}",
            parent.display()
        )));
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::new("output path has no valid file name"))?;
    let mut temp_path = None;
    let mut temp_file = None;
    for attempt in 0..100u32 {
        let candidate = parent.join(format!(
            ".{file_name}.tokito-{}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CliError::new(format!("cannot stage output: {error}"))),
        }
    }
    let temp_path =
        temp_path.ok_or_else(|| CliError::new("could not reserve a temporary output path"))?;
    let result = (|| {
        let mut file =
            temp_file.ok_or_else(|| CliError::new("temporary output file was not created"))?;
        file.write_all(contents)
            .map_err(|error| CliError::new(format!("cannot write staged output: {error}")))?;
        file.sync_all()
            .map_err(|error| CliError::new(format!("cannot flush staged output: {error}")))?;
        fs::hard_link(&temp_path, path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CliError::new(format!(
                    "output appeared during write; it was not overwritten: {}",
                    path.display()
                ))
            } else {
                CliError::new(format!(
                    "filesystem cannot atomically publish this output without replacement: {error}"
                ))
            }
        })?;
        let _ = fs::remove_file(&temp_path);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    block_len: usize,
    total_bytes: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            block_len: 0,
            total_bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(input.len() as u64);
        if self.block_len > 0 {
            let count = (64 - self.block_len).min(input.len());
            self.block[self.block_len..self.block_len + count].copy_from_slice(&input[..count]);
            self.block_len += count;
            input = &input[count..];
            if self.block_len < 64 {
                return;
            }
            sha256_compress(&mut self.state, &self.block);
            self.block_len = 0;
        }
        let mut full_blocks = input.chunks_exact(64);
        for bytes in &mut full_blocks {
            sha256_compress(
                &mut self.state,
                bytes.try_into().expect("exact SHA-256 block"),
            );
        }
        let remainder = full_blocks.remainder();
        self.block[..remainder.len()].copy_from_slice(remainder);
        self.block_len = remainder.len();
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_length = self.total_bytes.wrapping_mul(8);
        self.block[self.block_len] = 0x80;
        self.block_len += 1;
        if self.block_len > 56 {
            self.block[self.block_len..].fill(0);
            sha256_compress(&mut self.state, &self.block);
            self.block = [0; 64];
        } else {
            self.block[self.block_len..56].fill(0);
        }
        self.block[56..].copy_from_slice(&bit_length.to_be_bytes());
        sha256_compress(&mut self.state, &self.block);
        let mut digest = [0; 32];
        for (index, word) in self.state.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        digest
    }
}

fn sha256_compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut words = [0u32; 64];
    for (index, bytes) in block.chunks_exact(4).take(16).enumerate() {
        words[index] = u32::from_be_bytes(bytes.try_into().expect("four-byte word"));
    }
    for index in 16..64 {
        let x = words[index - 15];
        let y = words[index - 2];
        let small_sigma_0 = x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3);
        let small_sigma_1 = y.rotate_right(17) ^ y.rotate_right(19) ^ (y >> 10);
        words[index] = words[index - 16]
            .wrapping_add(small_sigma_0)
            .wrapping_add(words[index - 7])
            .wrapping_add(small_sigma_1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for index in 0..64 {
        let big_sigma_1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ ((!e) & g);
        let temp_1 = h
            .wrapping_add(big_sigma_1)
            .wrapping_add(choose)
            .wrapping_add(SHA256_ROUND_CONSTANTS[index])
            .wrapping_add(words[index]);
        let big_sigma_0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp_2 = big_sigma_0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp_1);
        d = c;
        c = b;
        b = a;
        a = temp_1.wrapping_add(temp_2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    digest_hex(&hasher.finalize())
}

fn digest_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn copy_bounded_file(
    source: &Path,
    destination: &Path,
    limit: usize,
) -> Result<(u64, String), CliError> {
    let input = fs::File::open(source).map_err(|error| {
        CliError::new(format!("cannot read source {}: {error}", source.display()))
    })?;
    let mut input = input.take(limit as u64 + 1);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| CliError::new(format!("cannot create package source file: {error}")))?;
    let mut hasher = Sha256::new();
    let mut bytes_written = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| CliError::new(format!("cannot read source board: {error}")))?;
        if count == 0 {
            break;
        }
        bytes_written += count as u64;
        if bytes_written > limit as u64 {
            return Err(CliError::new(format!(
                "source board exceeds the size limit ({limit} bytes) during package creation"
            )));
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| CliError::new(format!("cannot write package source: {error}")))?;
        hasher.update(&buffer[..count]);
    }
    output
        .sync_all()
        .map_err(|error| CliError::new(format!("cannot flush package source: {error}")))?;
    let digest = digest_hex(&hasher.finalize());
    Ok((bytes_written, digest))
}

fn json_escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output
}

fn inspect_json_for_source(board: &Board, source_label: &str) -> String {
    let footprints = board
        .footprints
        .iter()
        .map(|footprint| {
            format!(
                "{{\"reference\":\"{}\",\"reference_is_placeholder\":{},\"value\":\"{}\",\"library_id\":\"{}\",\"side\":\"{}\",\"x_mm\":{},\"y_mm\":{},\"rotation_deg\":{},\"uuid\":\"{}\",\"pad_count\":{},\"model_count\":{},\"dnp\":{},\"excluded_from_pos_files\":{}}}",
                json_escape(&footprint.reference),
                footprint.reference_is_placeholder,
                json_escape(&footprint.value),
                json_escape(&footprint.library_id),
                json_escape(&footprint.side),
                footprint.x_mm.map_or("null".to_owned(), |value| value.to_string()),
                footprint.y_mm.map_or("null".to_owned(), |value| value.to_string()),
                footprint
                    .rotation_deg
                    .map_or("null".to_owned(), |value| value.to_string()),
                json_escape(&footprint.uuid),
                footprint.pad_count,
                footprint.model_count,
                footprint.dnp,
                footprint.excluded_from_pos_files,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let layers = board
        .layers
        .iter()
        .map(|layer| format!("\"{}\"", json_escape(layer)))
        .collect::<Vec<_>>()
        .join(",");
    let mut all_diagnostics = board.diagnostics.clone();
    all_diagnostics.extend(board.geometry.diagnostics().iter().cloned());
    let diagnostics = all_diagnostics
        .iter()
        .map(|diagnostic| format!("\"{}\"", json_escape(diagnostic)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema\":\"tokito.board-inspection.v0\",\"source\":\"{}\",\"format\":\"kicad_pcb\",\"format_version\":{},\"units\":\"mm\",\"board_thickness_mm\":{},\"layers\":[{}],\"counts\":{{\"footprints\":{},\"pads\":{},\"models_referenced\":{},\"nets\":{},\"track_segments\":{},\"track_arcs\":{},\"vias\":{},\"zones\":{},\"edge_cuts_items\":{},\"parsed_svg_primitives\":{}}},\"footprints\":[{}],\"diagnostics\":[{}],\"geometry_export\":\"partial_svg\",\"geometry_complete\":false}}\n",
        json_escape(source_label),
        board.version,
        board
            .thickness_mm
            .map_or("null".to_owned(), |value| value.to_string()),
        layers,
        board.footprints.len(),
        board
            .footprints
            .iter()
            .map(|footprint| footprint.pad_count)
            .sum::<usize>(),
        board
            .footprints
            .iter()
            .map(|footprint| footprint.model_count)
            .sum::<usize>(),
        board.net_count,
        board.track_segment_count,
        board.track_arc_count,
        board.via_count,
        board.zone_count,
        board.edge_cuts_count,
        board.geometry.item_count(),
        footprints,
        diagnostics,
    )
}

fn inspect_json(board: &Board) -> String {
    let source_label = board
        .source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("board.kicad_pcb");
    inspect_json_for_source(board, source_label)
}

fn package_manifest(board: &Board, inspection: &str, placements: &str) -> String {
    let source_digest = format!("sha256:{}", board.source_sha256);
    let package_id = format!("kicad-{}", &board.source_sha256[..16]);
    format!(
        "{{\n  \"schema\": \"https://tokito.ai/schemas/board-package.schema.json\",\n  \"schemaVersion\": \"0.1.0\",\n  \"packageId\": \"{package_id}\",\n  \"packageLevel\": \"source-and-placement-inventory\",\n  \"mobileImportReady\": false,\n  \"source\": {{\"path\": \"source/board.kicad_pcb\", \"format\": \"kicad_pcb\", \"formatVersion\": {}, \"bytes\": {}, \"sha256\": \"{}\"}},\n  \"artifacts\": {{\"inspection\": {{\"path\": \"derived/inspection.json\", \"schema\": \"tokito.board-inspection.v0\", \"sha256\": \"sha256:{}\"}}, \"placements\": {{\"path\": \"derived/placements.csv\", \"rows\": {}, \"sha256\": \"sha256:{}\"}}}},\n  \"capabilities\": {{\"sourcePreserved\": true, \"placementInventory\": true, \"boardGeometryComplete\": false, \"svg\": false, \"glb\": false, \"step\": false}},\n  \"limitations\": [\"Inventory fields describe only the subset parsed by this Tokito CLI version.\", \"This package is not accepted by the Tokito App digital-twin runtime.\", \"No board mesh, component mesh, SVG, GLB, or STEP geometry is included.\", \"Source-adjacent license and provenance files are not discovered or copied automatically.\"],\n  \"producer\": {{\"name\": \"tokito-cli\", \"version\": \"0.1.0\"}}\n}}\n",
        board.version,
        board.source_bytes,
        source_digest,
        sha256_hex(inspection.as_bytes()),
        board.footprints.len(),
        sha256_hex(placements.as_bytes()),
    )
}

fn package_build(board: &Board, output: &Path) -> Result<(), CliError> {
    if output.exists() {
        return Err(CliError::new(format!(
            "package output already exists; choose another path: {}",
            output.display()
        )));
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(CliError::new(format!(
            "package output parent does not exist: {}",
            parent.display()
        )));
    }
    let output_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::new("package output path has no valid directory name"))?;
    let placements = placements_csv(board)?;
    let inspection = inspect_json_for_source(board, "board.kicad_pcb");
    let manifest = package_manifest(board, &inspection, &placements);

    let mut staging = None;
    for attempt in 0..100u32 {
        let candidate = parent.join(format!(
            ".{output_name}.tokito-{}-{attempt}.tmp",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                staging = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(CliError::new(format!(
                    "cannot stage package directory: {error}"
                )));
            }
        }
    }
    let staging =
        staging.ok_or_else(|| CliError::new("could not reserve a temporary package directory"))?;
    let result = (|| {
        let source_dir = staging.join("source");
        let derived_dir = staging.join("derived");
        fs::create_dir(&source_dir)
            .and_then(|_| fs::create_dir(&derived_dir))
            .map_err(|error| CliError::new(format!("cannot create package layout: {error}")))?;

        let source_path = source_dir.join("board.kicad_pcb");
        let (copied_bytes, copied_digest) =
            copy_bounded_file(&board.source, &source_path, MAX_INPUT_BYTES)?;
        if copied_bytes != board.source_bytes as u64 || copied_digest != board.source_sha256 {
            return Err(CliError::new(
                "source board changed during package creation; no package was published",
            ));
        }
        write_new_file(&derived_dir.join("inspection.json"), inspection.as_bytes())?;
        write_new_file(&derived_dir.join("placements.csv"), placements.as_bytes())?;
        write_new_file(&staging.join("manifest.json"), manifest.as_bytes())?;
        fs::create_dir(output).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CliError::new(format!(
                    "package output appeared during write; it was not overwritten: {}",
                    output.display()
                ))
            } else {
                CliError::new(format!("cannot reserve package output: {error}"))
            }
        })?;
        match fs::rename(&staging, output) {
            Ok(()) => Ok(()),
            Err(error) => {
                // This invocation created the empty destination reservation.
                // remove_dir refuses to remove it if another process populated it.
                let _ = fs::remove_dir(output);
                Err(CliError::new(format!(
                    "cannot publish package directory atomically: {error}"
                )))
            }
        }
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sparkfun_fixture() -> Board {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb",
        );
        load_board(&path).expect("pinned SparkFun KiCad board should parse")
    }

    fn pinned_fixture(relative_path: &str) -> Board {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_path);
        load_board(&path).expect("pinned open-source KiCad board should parse")
    }

    #[test]
    fn real_board_fixture_inspects_and_exports_uuid_backed_inventory() {
        let board = sparkfun_fixture();
        assert_eq!(board.version, 20241229);
        assert_eq!(board.footprints.len(), 20);
        assert_eq!(
            board
                .footprints
                .iter()
                .filter(|footprint| footprint.reference == "G***")
                .count(),
            3
        );
        assert!(
            !board
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("duplicate reference designator: G***"))
        );

        let csv = placements_csv(&board).expect("UUIDs disambiguate graphic placeholders");
        assert_eq!(csv.lines().count(), board.footprints.len() + 1);
        assert!(csv.starts_with("reference,reference_is_placeholder,"));
        assert_eq!(
            csv.lines()
                .filter(|line| line.starts_with("\"G***\",true,"))
                .count(),
            3
        );
        assert!(inspect_json(&board).contains("\"reference_is_placeholder\":true"));
    }

    #[test]
    fn pinned_routed_boards_import_and_export_closed_partial_svg_views() {
        let fixtures = [
            (
                "tests/fixtures/open-source/raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb",
                57,
                232,
                546,
                111,
                25,
            ),
            (
                "tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb",
                56,
                265,
                831,
                75,
                5,
            ),
            (
                "tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb",
                97,
                353,
                491,
                128,
                11,
            ),
        ];
        for (
            path,
            expected_footprints,
            expected_pads,
            expected_segments,
            expected_vias,
            expected_zones,
        ) in fixtures
        {
            let board = pinned_fixture(path);
            assert_eq!(board.footprints.len(), expected_footprints, "{path}");
            assert_eq!(
                board
                    .footprints
                    .iter()
                    .map(|footprint| footprint.pad_count)
                    .sum::<usize>(),
                expected_pads,
                "{path}"
            );
            assert_eq!(board.track_segment_count, expected_segments, "{path}");
            assert_eq!(board.via_count, expected_vias, "{path}");
            assert_eq!(board.zone_count, expected_zones, "{path}");

            let top = geometry::render_svg(&board.geometry, &board.layers, "top", None)
                .unwrap_or_else(|error| panic!("top view failed for {path}: {}", error.0));
            let bottom = geometry::render_svg(&board.geometry, &board.layers, "bottom", None)
                .unwrap_or_else(|error| panic!("bottom view failed for {path}: {}", error.0));
            assert!(top.contents.starts_with("<?xml version=\"1.0\""), "{path}");
            assert!(
                top.contents
                    .contains("data-coordinate-frame=\"KiCad-board\""),
                "{path}"
            );
            assert!(top.contents.contains("data-layer=\"F.Cu\""), "{path}");
            assert!(
                top.contents.contains("data-completeness=\"partial\""),
                "{path}"
            );
            assert!(
                bottom
                    .contents
                    .contains("data-view-transform=\"horizontal-mirror\""),
                "{path}"
            );
            assert!(top.rendered_items > 100, "{path}");
        }
    }

    #[test]
    fn sha256_matches_standard_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn package_build_writes_source_inventory_and_integrity_metadata() {
        let board = sparkfun_fixture();
        let test_root = std::env::temp_dir().join(format!(
            "tokito-cli-package-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        fs::create_dir(&test_root).expect("create isolated test directory");
        let package = test_root.join("fixture.tokito-board");
        package_build(&board, &package).expect("build source-and-inventory package");

        let source =
            fs::read(package.join("source/board.kicad_pcb")).expect("read packaged source");
        let inspection = fs::read_to_string(package.join("derived/inspection.json"))
            .expect("read packaged inspection");
        let placements = fs::read_to_string(package.join("derived/placements.csv"))
            .expect("read packaged placements");
        let manifest = fs::read_to_string(package.join("manifest.json")).expect("read manifest");
        assert_eq!(sha256_hex(&source), board.source_sha256);
        assert_eq!(placements.lines().count(), board.footprints.len() + 1);
        assert!(inspection.contains("\"source\":\"board.kicad_pcb\""));
        assert!(!inspection.contains(&board.source.to_string_lossy().to_string()));
        assert!(manifest.contains("\"schemaVersion\": \"0.1.0\""));
        assert!(manifest.contains("\"mobileImportReady\": false"));
        assert!(manifest.contains(&format!("\"sha256:{}\"", sha256_hex(inspection.as_bytes()))));
        assert!(manifest.contains(&format!("\"sha256:{}\"", sha256_hex(placements.as_bytes()))));
        fs::remove_dir_all(&test_root).expect("remove isolated test directory");
    }

    #[test]
    fn output_file_creation_does_not_replace_existing_content() {
        let test_root = std::env::temp_dir().join(format!(
            "tokito-cli-output-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        fs::create_dir(&test_root).expect("create isolated test directory");
        let destination = test_root.join("existing.txt");
        fs::write(&destination, b"user data").expect("create existing output");
        let error = write_new_file(&destination, b"replacement")
            .expect_err("existing output must not be replaced");
        assert!(error.0.contains("output already exists"));
        assert_eq!(
            fs::read(&destination).expect("read existing output"),
            b"user data"
        );
        fs::remove_dir_all(&test_root).expect("remove isolated test directory");
    }

    #[test]
    fn placement_export_still_rejects_duplicate_component_references() {
        let mut board = sparkfun_fixture();
        let duplicate_index = board
            .footprints
            .iter()
            .position(|footprint| footprint.reference == "C3")
            .expect("fixture has C3");
        board.footprints[duplicate_index].reference = "U2".to_owned();
        board.footprints[duplicate_index].reference_is_placeholder = false;

        let error = placements_csv(&board).expect_err("real component IDs must stay unique");
        assert!(
            error
                .0
                .contains("component reference designators are not unique")
        );
    }

    #[test]
    fn placement_export_rejects_duplicate_footprint_uuids() {
        let mut board = sparkfun_fixture();
        board.footprints[1].uuid = board.footprints[0].uuid.clone();

        let error = placements_csv(&board).expect_err("UUID is the stable row identity");
        assert!(error.0.contains("footprint UUIDs are not unique"));
    }
}
