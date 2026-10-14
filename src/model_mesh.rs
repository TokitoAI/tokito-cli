//! Bounded, original VRML97 reader for Group/Transform/DEF/USE/Shape triangle
//! and planar polygon IndexedFaceSet meshes. External resources, STEP,
//! scaleOrientation, textures, and general VRML evaluation are unsupported.
//! Source coordinates require an explicit unit declaration; output is local millimetres.
use crate::EngineError;
use earcut::Earcut;
use std::collections::{BTreeMap, BTreeSet};
mod zero_area;
pub use zero_area::derive_without_zero_area_faces;

const MAX_FACE_VERTICES: usize = 4096;
/// Deterministic aggregate proxy for quadratic polygon validation/Earcut work.
const MAX_POLYGON_TRIANGULATION_WORK: usize = 8_000_000;
const POLYGON_PLANAR_REL_TOLERANCE: f64 = 1e-8;
const POLYGON_AREA_REL_TOLERANCE: f64 = 1e-8;
const POLYGON_INTERSECTION_EPSILON: f64 = 64.0 * f64::EPSILON;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelUnits {
    Millimetres,
    /// KiCad legacy WRL library units: one deci-inch = 2.54 mm.
    KiCadLegacyWrlDeciinch,
}
impl ModelUnits {
    fn scale(self) -> f64 {
        match self {
            Self::Millimetres => 1.0,
            Self::KiCadLegacyWrlDeciinch => 2.54,
        }
    }
}
/// Caller limits may lower the default hard ceilings; increasing them is rejected.
/// Vertex, triangle, and quadratic polygon-triangulation work budgets
/// accumulate across the entire input.
#[derive(Debug, Clone, Copy)]
pub struct MeshLimits {
    pub max_bytes: usize,
    pub max_tokens: usize,
    pub max_depth: usize,
    pub max_shapes: usize,
    pub max_material_definitions: usize,
    pub max_vertices: usize,
    pub max_triangles: usize,
}
impl Default for MeshLimits {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_tokens: 500_000,
            max_depth: 16,
            max_shapes: 10_000,
            max_material_definitions: 1_000,
            max_vertices: 200_000,
            max_triangles: 200_000,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct MeshMaterial {
    pub diffuse_color: [f64; 3],
    pub specular_color: [f64; 3],
    pub emissive_color: [f64; 3],
    pub ambient_intensity: f64,
    pub shininess: f64,
    pub transparency: f64,
}
impl Default for MeshMaterial {
    fn default() -> Self {
        Self {
            diffuse_color: [0.8; 3],
            specular_color: [0.0; 3],
            emissive_color: [0.0; 3],
            ambient_intensity: 0.2,
            shininess: 0.2,
            transparency: 0.0,
        }
    }
}
#[derive(Debug, Clone)]
pub struct ModelMeshPart {
    pub source_shape_ordinal: usize,
    pub positions_mm: Vec<[f64; 3]>,
    /// Source triangle order, with winding normalized to counterclockwise.
    pub triangles: Vec<[u32; 3]>,
    /// One derived unit normal per triangle; these are not smoothed vertex normals.
    pub flat_normals: Vec<[f64; 3]>,
    pub material: Option<MeshMaterial>,
    pub source_crease_angle_radians: f64,
    pub solid: bool,
}
#[derive(Debug, Clone)]
pub struct ModelMesh {
    pub source_units: ModelUnits,
    pub root_shape_count: usize,
    pub material_definitions: BTreeMap<String, MeshMaterial>,
    pub parts: Vec<ModelMeshPart>,
    pub diagnostics: Vec<String>,
}
fn fail(message: impl Into<String>) -> EngineError {
    EngineError::new(format!("VRML mesh: {}", message.into()))
}

fn valid_def_identifier(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Parse only the declared bounded VRML97 profile. Group, Transform, DEF and
/// USE scene nodes may contain triangle or planar polygon Shape geometry; unsupported
/// fields/nodes (including scaleOrientation, Inline, normal, and texture data)
/// are errors. This API performs no filesystem or network access.
pub fn parse_vrml_mesh(
    input: &[u8],
    units: ModelUnits,
    limits: MeshLimits,
) -> Result<ModelMesh, EngineError> {
    parse_mesh_internal(input, units, limits, false)
}

fn parse_mesh_internal(
    input: &[u8],
    units: ModelUnits,
    limits: MeshLimits,
    topology_only: bool,
) -> Result<ModelMesh, EngineError> {
    let ceilings = MeshLimits::default();
    if limits.max_bytes > ceilings.max_bytes
        || limits.max_tokens > ceilings.max_tokens
        || limits.max_depth > ceilings.max_depth
        || limits.max_shapes > ceilings.max_shapes
        || limits.max_material_definitions > ceilings.max_material_definitions
        || limits.max_vertices > ceilings.max_vertices
        || limits.max_triangles > ceilings.max_triangles
    {
        return Err(fail("configured budget exceeds hard ceiling"));
    }
    if input.len() > limits.max_bytes {
        return Err(fail("byte budget exceeded"));
    }
    let text = std::str::from_utf8(input).map_err(|_| fail("input is not UTF-8"))?;
    if text.lines().next().map(str::trim_end) != Some("#VRML V2.0 utf8") {
        return Err(fail("expected VRML97 header #VRML V2.0 utf8"));
    }
    let tokens = tokenize(text, limits)?;
    let mut parser = Parser {
        tokens,
        cursor: 0,
        units,
        limits,
        materials: BTreeMap::new(),
        reserved_definition_names: BTreeSet::new(),
        scene_definitions: BTreeMap::new(),
        scene_definition_count: 0,
        vertices: 0,
        triangles: 0,
        remaining_polygon_work: MAX_POLYGON_TRIANGULATION_WORK,
        topology_only,
    };
    let mut parts = Vec::new();
    let mut shape_count = 0;
    while parser.cursor < parser.tokens.len() {
        parser.scene_node(0, MeshTransform::identity(), &mut parts, &mut shape_count)?;
    }
    if parts.is_empty() {
        return Err(fail("no nonempty triangle geometry"));
    }
    let diagnostics = vec![
        "Bounded VRML triangle/polygon profile with Group/Transform/DEF/USE scene support; local model geometry is not PCB placement or a complete CAD solid.".into(),
        "Flat triangle normals derived; creaseAngle is retained but smooth shading is not reproduced.".into(),
    ];
    Ok(ModelMesh {
        source_units: units,
        root_shape_count: shape_count,
        material_definitions: parser.materials,
        parts,
        diagnostics,
    })
}
fn tokenize(text: &str, limits: MeshLimits) -> Result<Vec<&str>, EngineError> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut stack = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        match bytes[at] {
            b'#' => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
                continue;
            }
            b' ' | b'\t' | b'\r' | b'\n' | b',' => {
                at += 1;
                continue;
            }
            b'{' | b'[' => {
                if stack.len() >= limits.max_depth {
                    return Err(fail("depth budget exceeded"));
                }
                stack.push(bytes[at]);
                at += 1;
            }
            b'}' | b']' => {
                let expected = if bytes[at] == b'}' { b'{' } else { b'[' };
                if stack.pop() != Some(expected) {
                    return Err(fail("unbalanced delimiters"));
                }
                at += 1;
            }
            _ => {
                while at < bytes.len()
                    && !matches!(
                        bytes[at],
                        b' ' | b'\t' | b'\r' | b'\n' | b',' | b'#' | b'{' | b'}' | b'[' | b']'
                    )
                {
                    if at - start >= 256 {
                        return Err(fail("token byte budget exceeded"));
                    }
                    if !bytes[at].is_ascii() || bytes[at] == b'"' || bytes[at].is_ascii_control() {
                        return Err(fail("unsupported token or quoted string"));
                    }
                    at += 1;
                }
            }
        }
        if tokens.len() >= limits.max_tokens {
            return Err(fail("token budget exceeded"));
        }
        tokens.push(&text[start..at]);
    }
    if !stack.is_empty() {
        return Err(fail("unclosed delimiters"));
    }
    Ok(tokens)
}

/// Explicit, bounded derivation for the flat WRL profile. Original bytes stay
/// untouched. It removes only appearance-only root Shapes whose never-used
/// material is outside the strict parser's numeric ranges. Other unused palette
/// entries, geometry-bearing Shapes and used definitions stay byte-exact.
pub fn derive_without_unused_invalid_materials(
    input: &[u8],
    units: ModelUnits,
) -> Result<(Vec<u8>, serde_json::Value), EngineError> {
    let (derived, mut report) = palette_derivation(input)?;
    let mesh = parse_vrml_mesh(&derived, units, MeshLimits::default())?;
    report["derivedMeshParts"] = serde_json::json!(mesh.parts.len());
    report["derivedTriangles"] =
        serde_json::json!(mesh.parts.iter().map(|p| p.triangles.len()).sum::<usize>());
    Ok((derived, report))
}

fn palette_derivation(input: &[u8]) -> Result<(Vec<u8>, serde_json::Value), EngineError> {
    let limits = MeshLimits::default();
    if input.len() > limits.max_bytes {
        return Err(fail("byte budget exceeded"));
    }
    let text = std::str::from_utf8(input).map_err(|_| fail("input is not UTF-8"))?;
    if text.lines().next().map(str::trim_end) != Some("#VRML V2.0 utf8") {
        return Err(fail("expected VRML97 header"));
    }
    let tokens = tokenize(text, limits)?;
    let mut defined = BTreeSet::new();
    let mut used = BTreeSet::new();
    for (i, token) in tokens.iter().enumerate() {
        if *token == "DEF" {
            let id = *tokens.get(i + 1).ok_or_else(|| fail("missing DEF name"))?;
            if tokens.get(i + 2) != Some(&"Material") || !defined.insert(id) {
                return Err(fail("derivation requires unique Material definitions"));
            }
            if defined.len() > limits.max_material_definitions {
                return Err(fail("material definition budget exceeded"));
            }
        } else if *token == "USE" {
            used.insert(*tokens.get(i + 1).ok_or_else(|| fail("missing USE name"))?);
        }
    }
    if !used.is_subset(&defined) {
        return Err(fail("undefined USE in source"));
    }
    let mut cursor = 0;
    let mut shapes = 0;
    let mut spans = Vec::new();
    let mut removals = Vec::new();
    while cursor < tokens.len() {
        if shapes >= limits.max_shapes {
            return Err(fail("shape budget exceeded"));
        }
        let start = cursor;
        if tokens.get(start) != Some(&"Shape") || tokens.get(start + 1) != Some(&"{") {
            return Err(fail("derivation requires flat root Shapes"));
        }
        cursor += 2;
        let mut depth = 1;
        while depth > 0 {
            match *tokens
                .get(cursor)
                .ok_or_else(|| fail("unclosed root Shape"))?
            {
                "{" => depth += 1,
                "}" => depth -= 1,
                _ => (),
            }
            cursor += 1;
        }
        let shape = &tokens[start..cursor];
        let prefix = [
            "Shape",
            "{",
            "appearance",
            "Appearance",
            "{",
            "material",
            "DEF",
        ];
        if shape.starts_with(&prefix)
            && shape.len() >= 13
            && shape[8..10] == ["Material", "{"]
            && shape[shape.len() - 3..] == ["}", "}", "}"]
            && !used.contains(shape[7])
        {
            // Strict grammar for an unused numeric Material definition. No
            // geometry, ROUTE, nested node or unknown field can disappear.
            let mut at = 10;
            let end = shape.len() - 3;
            let mut fields = BTreeSet::new();
            let mut values_supported = true;
            while at < end {
                let field = shape[at];
                at += 1;
                let count = match field {
                    "ambientIntensity" | "shininess" | "transparency" => 1,
                    "diffuseColor" | "specularColor" | "emissiveColor" => 3,
                    _ => return Err(fail("unsupported unused palette field")),
                };
                if !fields.insert(field) || at + count > end {
                    return Err(fail("malformed unused palette Material"));
                }
                for token in &shape[at..at + count] {
                    let value = token
                        .parse::<f64>()
                        .map_err(|_| fail("unused palette value must be numeric"))?;
                    values_supported &= value.is_finite() && (0.0..=1.0).contains(&value);
                }
                at += count;
            }
            if !values_supported {
                let begin = shape[0].as_ptr() as usize - text.as_ptr() as usize;
                let finish = shape.last().unwrap().as_ptr() as usize - text.as_ptr() as usize + 1;
                spans.push((begin, finish));
                removals.push(serde_json::json!({"materialDef":shape[7],"sourceRootShapeOrdinal":shapes,"startByte":begin,"endByteExclusive":finish,"removedBytesSha256":crate::sha256_hex(&input[begin..finish]),"useCount":0}));
            }
        }
        shapes += 1;
    }
    if spans.is_empty() {
        return Err(fail("no unused out-of-range palette Material Shapes"));
    }
    let mut derived = Vec::with_capacity(input.len());
    let mut begin = 0;
    for (start, end) in &spans {
        derived.extend_from_slice(&input[begin..*start]);
        begin = *end;
    }
    derived.extend_from_slice(&input[begin..]);
    let report = serde_json::json!({"schema":"tokito.vrml-unused-invalid-material-derivation.v1","sourceSha256":crate::sha256_hex(input),"derivedSha256":crate::sha256_hex(&derived),"sourceBytes":input.len(),"derivedBytes":derived.len(),"removedPaletteShapes":removals,"retainedBytesUnchanged":true,"geometryModified":false,"usedMaterialDefinitionsModified":false,"physicalFitVerified":false,"placementVerified":false,"cadSolidVerified":false});
    Ok((derived, report))
}

struct Parser<'a> {
    tokens: Vec<&'a str>,
    cursor: usize,
    units: ModelUnits,
    limits: MeshLimits,
    materials: BTreeMap<String, MeshMaterial>,
    reserved_definition_names: BTreeSet<String>,
    scene_definitions: BTreeMap<String, SceneDefinition>,
    scene_definition_count: usize,
    vertices: usize,
    triangles: usize,
    remaining_polygon_work: usize,
    topology_only: bool,
}

#[derive(Clone)]
struct SceneDefinition {
    shape_start_ordinal: usize,
    shape_count: usize,
    parts: Vec<ModelMeshPart>,
}

/// Affine transform in millimetres. `compose` returns `self * inner`, so the
/// inner node is applied first, matching nested VRML Transform semantics.
#[derive(Clone, Copy)]
struct MeshTransform {
    linear: [[f64; 3]; 3],
    translation: [f64; 3],
    reverses_winding: bool,
}
impl MeshTransform {
    fn identity() -> Self {
        Self {
            linear: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            translation: [0.0; 3],
            reverses_winding: false,
        }
    }
    fn is_identity(self) -> bool {
        self.linear == Self::identity().linear
            && self.translation == [0.0; 3]
            && !self.reverses_winding
    }
    fn apply(self, point: [f64; 3]) -> [f64; 3] {
        let mut result = self.translation;
        for (row, value) in result.iter_mut().enumerate() {
            for (column, coordinate) in point.iter().enumerate() {
                *value += self.linear[row][column] * coordinate;
            }
        }
        result
    }
    fn compose(self, inner: Self) -> Self {
        let mut linear = [[0.0; 3]; 3];
        for (row, values) in linear.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().enumerate() {
                for middle in 0..3 {
                    *value += self.linear[row][middle] * inner.linear[middle][column];
                }
            }
        }
        let mut translation = self.translation;
        for (row, value) in translation.iter_mut().enumerate() {
            for column in 0..3 {
                *value += self.linear[row][column] * inner.translation[column];
            }
        }
        Self {
            linear,
            translation,
            reverses_winding: self.reverses_winding ^ inner.reverses_winding,
        }
    }
    fn from_vrml(
        translation: [f64; 3],
        rotation: [f64; 4],
        scale: [f64; 3],
        center: [f64; 3],
    ) -> Result<Self, EngineError> {
        let [axis_x, axis_y, axis_z, angle] = rotation;
        let axis_length = axis_x.hypot(axis_y).hypot(axis_z);
        if !axis_length.is_finite() || axis_length == 0.0 {
            return Err(fail("Transform rotation axis must be nonzero"));
        }
        if scale.contains(&0.0) {
            return Err(fail("Transform scale components must be nonzero"));
        }
        let [x, y, z] = [
            axis_x / axis_length,
            axis_y / axis_length,
            axis_z / axis_length,
        ];
        let (sin, cos) = angle.sin_cos();
        let rotation_matrix = [
            [
                cos + x * x * (1.0 - cos),
                x * y * (1.0 - cos) - z * sin,
                x * z * (1.0 - cos) + y * sin,
            ],
            [
                y * x * (1.0 - cos) + z * sin,
                cos + y * y * (1.0 - cos),
                y * z * (1.0 - cos) - x * sin,
            ],
            [
                z * x * (1.0 - cos) - y * sin,
                z * y * (1.0 - cos) + x * sin,
                cos + z * z * (1.0 - cos),
            ],
        ];
        let mut linear = rotation_matrix;
        for values in &mut linear {
            for (column, value) in values.iter_mut().enumerate() {
                *value *= scale[column];
            }
            if values.iter().any(|value| !value.is_finite()) {
                return Err(fail("Transform matrix overflow"));
            }
        }
        let mut offset = translation;
        for row in 0..3 {
            offset[row] += center[row];
            for column in 0..3 {
                offset[row] -= linear[row][column] * center[column];
            }
        }
        if offset.iter().any(|value| !value.is_finite()) {
            return Err(fail("Transform translation overflow"));
        }
        Ok(Self {
            linear,
            translation: offset,
            reverses_winding: scale.iter().filter(|axis| axis.is_sign_negative()).count() % 2 == 1,
        })
    }
}

impl<'a> Parser<'a> {
    fn take(&mut self) -> Result<&'a str, EngineError> {
        let value = self
            .tokens
            .get(self.cursor)
            .copied()
            .ok_or_else(|| fail("unexpected end"))?;
        self.cursor += 1;
        Ok(value)
    }
    fn expect(&mut self, expected: &'static str) -> Result<(), EngineError> {
        let actual = self.take()?;
        if actual != expected {
            return Err(fail(format!("expected {expected}")));
        }
        Ok(())
    }
    fn closing(&mut self, close: &str) -> bool {
        if self.tokens.get(self.cursor) == Some(&close) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }
    fn number(&mut self) -> Result<f64, EngineError> {
        let token = self.take()?;
        let value: f64 = token.parse().map_err(|_| fail("invalid number"))?;
        if !value.is_finite() {
            return Err(fail("nonfinite number"));
        }
        Ok(value)
    }
    fn unit_number(&mut self) -> Result<f64, EngineError> {
        let value = self.number()?;
        if !(0.0..=1.0).contains(&value) {
            return Err(fail("material value outside [0,1]"));
        }
        Ok(value)
    }
    fn color(&mut self) -> Result<[f64; 3], EngineError> {
        Ok([
            self.unit_number()?,
            self.unit_number()?,
            self.unit_number()?,
        ])
    }
    fn boolean(&mut self) -> Result<bool, EngineError> {
        match self.take()? {
            "TRUE" => Ok(true),
            "FALSE" => Ok(false),
            _ => Err(fail("expected TRUE or FALSE")),
        }
    }
    fn vector3(&mut self) -> Result<[f64; 3], EngineError> {
        let values = [self.number()?, self.number()?, self.number()?];
        let values = values.map(|value| value * self.units.scale());
        if values.iter().any(|value| !value.is_finite()) {
            return Err(fail("Transform unit conversion overflow"));
        }
        Ok(values)
    }
    fn scene_node(
        &mut self,
        depth: usize,
        transform: MeshTransform,
        parts: &mut Vec<ModelMeshPart>,
        shape_count: &mut usize,
    ) -> Result<(), EngineError> {
        if depth > self.limits.max_depth {
            return Err(fail("scene node depth budget exceeded"));
        }
        let node = self.take()?;
        if node == "DEF" {
            let name = self.take()?.to_owned();
            if !valid_def_identifier(&name) {
                return Err(fail("unsupported scene DEF identifier"));
            }
            if !self.reserved_definition_names.insert(name.clone()) {
                return Err(fail("duplicate VRML DEF identifier"));
            }
            if self.scene_definition_count >= self.limits.max_shapes {
                return Err(fail("scene definition budget exceeded"));
            }
            let shape_start_ordinal = *shape_count;
            let mut definition_parts = Vec::new();
            self.scene_node(
                depth + 1,
                MeshTransform::identity(),
                &mut definition_parts,
                shape_count,
            )?;
            let definition_shape_count = shape_count
                .checked_sub(shape_start_ordinal)
                .ok_or_else(|| fail("invalid scene definition shape accounting"))?;
            self.scene_definitions.insert(
                name,
                SceneDefinition {
                    shape_start_ordinal,
                    shape_count: definition_shape_count,
                    parts: definition_parts.clone(),
                },
            );
            self.scene_definition_count += 1;
            for mut part in definition_parts {
                transform_part(&mut part, transform, self.topology_only)?;
                parts.push(part);
            }
            return Ok(());
        }
        if node == "USE" {
            let name = self.take()?;
            let definition = self
                .scene_definitions
                .get(name)
                .cloned()
                .ok_or_else(|| fail("undefined scene USE"))?;
            let additional_vertices = definition
                .parts
                .iter()
                .try_fold(0usize, |total, part| {
                    total.checked_add(part.positions_mm.len())
                })
                .ok_or_else(|| fail("scene USE vertex count overflow"))?;
            self.vertices = self
                .vertices
                .checked_add(additional_vertices)
                .filter(|total| *total <= self.limits.max_vertices)
                .ok_or_else(|| fail("vertex budget exceeded by scene USE"))?;
            let additional_triangles = definition
                .parts
                .iter()
                .try_fold(0usize, |total, part| {
                    total.checked_add(part.triangles.len())
                })
                .ok_or_else(|| fail("scene USE triangle count overflow"))?;
            self.triangles = self
                .triangles
                .checked_add(additional_triangles)
                .filter(|total| *total <= self.limits.max_triangles)
                .ok_or_else(|| fail("triangle budget exceeded by scene USE"))?;
            let instance_shape_start = *shape_count;
            let next_shape_count = shape_count
                .checked_add(definition.shape_count)
                .filter(|total| *total <= self.limits.max_shapes)
                .ok_or_else(|| fail("shape budget exceeded by scene USE"))?;
            for mut part in definition.parts {
                let relative_ordinal = part
                    .source_shape_ordinal
                    .checked_sub(definition.shape_start_ordinal)
                    .ok_or_else(|| fail("invalid scene USE shape accounting"))?;
                part.source_shape_ordinal = instance_shape_start
                    .checked_add(relative_ordinal)
                    .ok_or_else(|| fail("scene USE shape ordinal overflow"))?;
                transform_part(&mut part, transform, self.topology_only)?;
                parts.push(part);
            }
            *shape_count = next_shape_count;
            return Ok(());
        }
        match node {
            "Shape" => {
                if *shape_count >= self.limits.max_shapes {
                    return Err(fail("shape budget exceeded"));
                }
                let ordinal = *shape_count;
                *shape_count += 1;
                if let Some(mut part) = self.shape(ordinal)? {
                    transform_part(&mut part, transform, self.topology_only)?;
                    parts.push(part);
                }
                Ok(())
            }
            "Group" => {
                self.expect("{")?;
                let mut seen = BTreeSet::new();
                while !self.closing("}") {
                    match self.field(&mut seen)? {
                        "children" => self.children(depth, transform, parts, shape_count)?,
                        _ => return Err(fail("unsupported Group field")),
                    }
                }
                Ok(())
            }
            "Transform" => {
                self.expect("{")?;
                let mut seen = BTreeSet::new();
                let mut translation = [0.0; 3];
                let mut rotation = [0.0, 0.0, 1.0, 0.0];
                let mut scale = [1.0; 3];
                let mut center = [0.0; 3];
                let mut child_parts = Vec::new();
                while !self.closing("}") {
                    match self.field(&mut seen)? {
                        "translation" => translation = self.vector3()?,
                        "rotation" => {
                            rotation = [
                                self.number()?,
                                self.number()?,
                                self.number()?,
                                self.number()?,
                            ]
                        }
                        "scale" => scale = [self.number()?, self.number()?, self.number()?],
                        "center" => center = self.vector3()?,
                        "children" => self.children(
                            depth,
                            MeshTransform::identity(),
                            &mut child_parts,
                            shape_count,
                        )?,
                        "scaleOrientation" => {
                            return Err(fail("unsupported Transform scaleOrientation"));
                        }
                        _ => return Err(fail("unsupported Transform field")),
                    }
                }
                let local = MeshTransform::from_vrml(translation, rotation, scale, center)?;
                let composed = transform.compose(local);
                for mut part in child_parts {
                    transform_part(&mut part, composed, self.topology_only)?;
                    parts.push(part);
                }
                Ok(())
            }
            _ => Err(fail("unsupported scene node")),
        }
    }
    fn children(
        &mut self,
        parent_depth: usize,
        transform: MeshTransform,
        parts: &mut Vec<ModelMeshPart>,
        shape_count: &mut usize,
    ) -> Result<(), EngineError> {
        self.expect("[")?;
        while !self.closing("]") {
            self.scene_node(parent_depth + 1, transform, parts, shape_count)?;
        }
        Ok(())
    }
    fn field(&mut self, seen: &mut BTreeSet<String>) -> Result<&'a str, EngineError> {
        let field = self.take()?;
        if !seen.insert(field.to_string()) {
            return Err(fail("duplicate field"));
        }
        Ok(field)
    }
    fn shape(&mut self, ordinal: usize) -> Result<Option<ModelMeshPart>, EngineError> {
        self.expect("{")?;
        let mut seen = BTreeSet::new();
        let mut material = None;
        let mut geometry = None;
        while !self.closing("}") {
            match self.field(&mut seen)? {
                "appearance" => material = self.appearance()?,
                "geometry" => geometry = Some(self.geometry(ordinal)?),
                _ => return Err(fail("unsupported Shape field")),
            }
        }
        if let Some(mut part) = geometry {
            part.material = material;
            Ok(Some(part))
        } else {
            Ok(None)
        }
    }
    fn appearance(&mut self) -> Result<Option<MeshMaterial>, EngineError> {
        self.expect("Appearance")?;
        self.expect("{")?;
        let mut seen = BTreeSet::new();
        let mut material = None;
        while !self.closing("}") {
            match self.field(&mut seen)? {
                "material" => material = Some(self.material()?),
                _ => return Err(fail("unsupported Appearance field")),
            }
        }
        Ok(material)
    }
    fn material(&mut self) -> Result<MeshMaterial, EngineError> {
        let node = self.take()?;
        if node == "USE" {
            let name = self.take()?;
            return self
                .materials
                .get(name)
                .cloned()
                .ok_or_else(|| fail("undefined material USE"));
        }
        let definition = if node == "DEF" {
            let name = self.take()?;
            if !valid_def_identifier(name) {
                return Err(fail("unsupported material DEF identifier"));
            }
            if self.materials.contains_key(name) {
                return Err(fail("duplicate material DEF"));
            }
            if !self.reserved_definition_names.insert(name.to_owned()) {
                return Err(fail("duplicate VRML DEF identifier"));
            }
            if self.materials.len() >= self.limits.max_material_definitions {
                return Err(fail("material definition budget exceeded"));
            }
            self.expect("Material")?;
            Some(name.to_string())
        } else if node == "Material" {
            None
        } else {
            return Err(fail("unsupported material node"));
        };
        self.expect("{")?;
        let mut seen = BTreeSet::new();
        let mut material = MeshMaterial::default();
        while !self.closing("}") {
            match self.field(&mut seen)? {
                "diffuseColor" => material.diffuse_color = self.color()?,
                "specularColor" => material.specular_color = self.color()?,
                "emissiveColor" => material.emissive_color = self.color()?,
                "ambientIntensity" => material.ambient_intensity = self.unit_number()?,
                "shininess" => material.shininess = self.unit_number()?,
                "transparency" => material.transparency = self.unit_number()?,
                _ => return Err(fail("unsupported Material field")),
            }
        }
        if let Some(name) = definition {
            self.materials.insert(name, material.clone());
        }
        Ok(material)
    }
    fn coordinates(&mut self) -> Result<Vec<[f64; 3]>, EngineError> {
        self.expect("Coordinate")?;
        self.expect("{")?;
        self.expect("point")?;
        self.expect("[")?;
        let mut positions = Vec::new();
        while !self.closing("]") {
            if self.vertices >= self.limits.max_vertices {
                return Err(fail("vertex budget exceeded"));
            }
            let point = [
                self.number()? * self.units.scale(),
                self.number()? * self.units.scale(),
                self.number()? * self.units.scale(),
            ];
            if point.iter().any(|v| !v.is_finite()) {
                return Err(fail("unit conversion overflow"));
            }
            positions.push(point);
            self.vertices += 1;
        }
        self.expect("}")?;
        Ok(positions)
    }
    fn indices(&mut self) -> Result<Vec<Vec<u32>>, EngineError> {
        self.expect("[")?;
        let mut faces = Vec::new();
        let mut face = Vec::new();
        while !self.closing("]") {
            let token = self.take()?;
            let index: i64 = token
                .parse()
                .map_err(|_| fail("coordIndex must contain integers"))?;
            if index == -1 {
                if face.len() < 3 {
                    return Err(fail(
                        "IndexedFaceSet face must contain at least three vertices (triangular minimum)",
                    ));
                }
                faces.push(std::mem::take(&mut face));
            } else {
                if index < 0 || index > u32::MAX as i64 {
                    return Err(fail("invalid coordinate index"));
                }
                if face.len() >= MAX_FACE_VERTICES {
                    return Err(fail("polygon face vertex budget exceeded"));
                }
                face.push(index as u32);
            }
        }
        if !face.is_empty() {
            return Err(fail("face missing -1 terminator"));
        }
        Ok(faces)
    }
    fn geometry(&mut self, ordinal: usize) -> Result<ModelMeshPart, EngineError> {
        self.expect("IndexedFaceSet")?;
        self.expect("{")?;
        let mut seen = BTreeSet::new();
        let mut positions = None;
        let mut faces = None;
        let mut crease_angle = 0.0;
        let mut ccw = true;
        let mut solid = true;
        while !self.closing("}") {
            match self.field(&mut seen)? {
                "coord" => positions = Some(self.coordinates()?),
                "coordIndex" => faces = Some(self.indices()?),
                "creaseAngle" => {
                    crease_angle = self.number()?;
                    if crease_angle < 0.0 {
                        return Err(fail("negative creaseAngle"));
                    }
                }
                "ccw" => ccw = self.boolean()?,
                "solid" => solid = self.boolean()?,
                _ => return Err(fail("unsupported IndexedFaceSet field")),
            }
        }
        let positions = positions.ok_or_else(|| fail("missing Coordinate points"))?;
        let faces = faces.ok_or_else(|| fail("missing coordIndex"))?;
        let triangle_budget = self
            .limits
            .max_triangles
            .checked_sub(self.triangles)
            .ok_or_else(|| fail("triangle budget exceeded"))?;
        let mut triangles = triangulate_indexed_faces(
            &faces,
            &positions,
            triangle_budget,
            &mut self.remaining_polygon_work,
        )?;
        self.triangles = self
            .triangles
            .checked_add(triangles.len())
            .filter(|total| *total <= self.limits.max_triangles)
            .ok_or_else(|| fail("triangle budget exceeded"))?;
        if positions.is_empty() || triangles.is_empty() {
            return Err(fail("empty mesh geometry"));
        }
        let mut normals = Vec::with_capacity(triangles.len());
        for triangle in &mut triangles {
            if triangle
                .iter()
                .any(|&index| index as usize >= positions.len())
            {
                return Err(fail("coordinate index out of range"));
            }
            if !ccw {
                triangle.swap(1, 2);
            }
            normals.push(if self.topology_only {
                [0.0, 0.0, 1.0]
            } else {
                normal(
                    positions[triangle[0] as usize],
                    positions[triangle[1] as usize],
                    positions[triangle[2] as usize],
                )?
            });
        }
        Ok(ModelMeshPart {
            source_shape_ordinal: ordinal,
            positions_mm: positions,
            triangles,
            flat_normals: normals,
            material: None,
            source_crease_angle_radians: crease_angle,
            solid,
        })
    }
}

/// Triangulate each VRML IndexedFaceSet polygon independently. The earcut
/// output is admitted only when it preserves the projected source area and
/// every source polygon is planar within a scale-relative bound.
fn triangulate_indexed_faces(
    faces: &[Vec<u32>],
    positions: &[[f64; 3]],
    max_triangles: usize,
    remaining_polygon_work: &mut usize,
) -> Result<Vec<[u32; 3]>, EngineError> {
    let mut triangles = Vec::new();
    for face in faces {
        if face.len() < 3 || face.len() > MAX_FACE_VERTICES {
            return Err(fail("polygon face vertex budget exceeded"));
        }
        let required = face
            .len()
            .checked_sub(2)
            .ok_or_else(|| fail("invalid polygon face size"))?;
        if triangles
            .len()
            .checked_add(required)
            .is_none_or(|count| count > max_triangles)
        {
            return Err(fail("triangle budget exceeded"));
        }
        if face.len() == 3 {
            let triangle = [face[0], face[1], face[2]];
            if triangle[0] == triangle[1]
                || triangle[1] == triangle[2]
                || triangle[2] == triangle[0]
            {
                return Err(fail(
                    "polygon face repeats a coordinate index; triangular/polygon topology must use distinct indices",
                ));
            }
            if triangle
                .iter()
                .any(|index| *index as usize >= positions.len())
            {
                return Err(fail("coordinate index out of range"));
            }
            triangles.push(triangle);
            continue;
        }
        // Account for both the non-adjacent edge intersection scan and the
        // triangulator's quadratic worst-case work before doing either.
        let polygon_work = face
            .len()
            .checked_mul(face.len())
            .and_then(|work| work.checked_mul(2))
            .ok_or_else(|| fail("polygon triangulation work budget exceeded"))?;
        *remaining_polygon_work = remaining_polygon_work
            .checked_sub(polygon_work)
            .ok_or_else(|| fail("polygon triangulation work budget exceeded"))?;

        let mut seen = BTreeSet::new();
        let points = face
            .iter()
            .map(|&index| {
                if !seen.insert(index) {
                    return Err(fail(
                        "polygon face repeats a coordinate index; triangular/polygon topology must use distinct indices",
                    ));
                }
                positions
                    .get(index as usize)
                    .copied()
                    .ok_or_else(|| fail("coordinate index out of range"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let normal = polygon_plane_normal(&points)?;
        let anchor = points[0];
        let mut local_scale = 0.0_f64;
        let relative_points = points
            .iter()
            .map(|point| {
                let delta = [
                    point[0] - anchor[0],
                    point[1] - anchor[1],
                    point[2] - anchor[2],
                ];
                if delta.iter().any(|value| !value.is_finite()) {
                    return Err(fail("polygon coordinate difference overflow"));
                }
                local_scale =
                    local_scale.max(delta.iter().map(|value| value.abs()).fold(0.0, f64::max));
                Ok(delta)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !local_scale.is_finite() || local_scale == 0.0 {
            return Err(fail("degenerate polygon face"));
        }
        let planarity_tolerance = (local_scale * POLYGON_PLANAR_REL_TOLERANCE).max(1e-10);
        for delta in &relative_points {
            let distance = delta[0] * normal[0] + delta[1] * normal[1] + delta[2] * normal[2];
            if !distance.is_finite() || distance.abs() > planarity_tolerance {
                return Err(fail("non-planar IndexedFaceSet polygon"));
            }
        }

        let drop_axis = (0..3)
            .max_by(|left, right| normal[*left].abs().total_cmp(&normal[*right].abs()))
            .ok_or_else(|| fail("cannot select polygon projection plane"))?;
        let axes = match drop_axis {
            0 => [1, 2],
            1 => [0, 2],
            _ => [0, 1],
        };
        let projected = relative_points
            .iter()
            .map(|point| [point[axes[0]] / local_scale, point[axes[1]] / local_scale])
            .collect::<Vec<_>>();
        validate_simple_polygon(&projected)?;
        let mut signed_area = 0.0;
        for index in 0..projected.len() {
            let current = projected[index];
            let next = projected[(index + 1) % projected.len()];
            if current == next {
                return Err(fail("polygon face has a repeated projected point"));
            }
            signed_area += current[0] * next[1] - current[1] * next[0];
        }
        signed_area *= 0.5;
        if !signed_area.is_finite() || signed_area.abs() <= f64::EPSILON {
            return Err(fail("degenerate projected polygon face"));
        }

        let mut local_triangles = Vec::<u32>::new();
        Earcut::<f64>::new().earcut(projected.iter().copied(), &[], &mut local_triangles);
        if local_triangles.len() != required * 3 {
            return Err(fail("polygon triangulation did not preserve face topology"));
        }
        let mut triangulated_area = 0.0;
        for triangle in local_triangles.chunks_exact_mut(3) {
            let indices = [
                triangle[0] as usize,
                triangle[1] as usize,
                triangle[2] as usize,
            ];
            if indices.iter().any(|index| *index >= projected.len())
                || indices[0] == indices[1]
                || indices[1] == indices[2]
                || indices[2] == indices[0]
            {
                return Err(fail("polygon triangulator returned an invalid index"));
            }
            let a = projected[indices[0]];
            let b = projected[indices[1]];
            let c = projected[indices[2]];
            let twice_area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if !twice_area.is_finite() || twice_area.abs() <= f64::EPSILON {
                return Err(fail("polygon triangulator returned a degenerate triangle"));
            }
            triangulated_area += twice_area.abs() * 0.5;
            if twice_area.is_sign_positive() != signed_area.is_sign_positive() {
                triangle.swap(1, 2);
            }
            triangles.push([face[indices[0]], face[indices[1]], face[indices[2]]]);
        }
        let area_tolerance = (signed_area.abs() * POLYGON_AREA_REL_TOLERANCE).max(1e-12);
        if (triangulated_area - signed_area.abs()).abs() > area_tolerance {
            return Err(fail(
                "polygon triangulation area does not match source face",
            ));
        }
    }
    Ok(triangles)
}

/// Refuse non-simple polygon rings before tessellation. Non-adjacent edges may
/// not cross or touch; adjacent collinear edges may meet at their shared
/// vertex, but may not overlap beyond it.
fn validate_simple_polygon(points: &[[f64; 2]]) -> Result<(), EngineError> {
    if points.len() < 3 {
        return Err(fail("polygon face must contain at least three points"));
    }
    for (left, a) in points.iter().enumerate() {
        let left_next = (left + 1) % points.len();
        let b = points[left_next];
        for right in left + 1..points.len() {
            let right_next = (right + 1) % points.len();
            let c = points[right];
            let d = points[right_next];
            let adjacent = left_next == right || right_next == left;
            if adjacent {
                let (shared, first_other, second_other) = if left_next == right {
                    (b, *a, d)
                } else {
                    (*a, c, b)
                };
                let first = [first_other[0] - shared[0], first_other[1] - shared[1]];
                let second = [second_other[0] - shared[0], second_other[1] - shared[1]];
                let cross = first[0] * second[1] - first[1] * second[0];
                let dot = first[0] * second[0] + first[1] * second[1];
                if cross.abs() <= POLYGON_INTERSECTION_EPSILON && dot > 0.0 {
                    return Err(fail("polygon ring has overlapping adjacent edges"));
                }
                continue;
            }
            if segments_intersect_or_touch(*a, b, c, d) {
                return Err(fail("self-intersecting or self-touching polygon ring"));
            }
        }
    }
    Ok(())
}

fn segments_intersect_or_touch(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let ab_c = orient2(a, b, c);
    let ab_d = orient2(a, b, d);
    let cd_a = orient2(c, d, a);
    let cd_b = orient2(c, d, b);
    let sign = |value: f64| {
        if value > POLYGON_INTERSECTION_EPSILON {
            1
        } else if value < -POLYGON_INTERSECTION_EPSILON {
            -1
        } else {
            0
        }
    };
    let (ab_c, ab_d, cd_a, cd_b) = (sign(ab_c), sign(ab_d), sign(cd_a), sign(cd_b));
    if ab_c * ab_d < 0 && cd_a * cd_b < 0 {
        return true;
    }
    (ab_c == 0 && point_on_segment(c, a, b))
        || (ab_d == 0 && point_on_segment(d, a, b))
        || (cd_a == 0 && point_on_segment(a, c, d))
        || (cd_b == 0 && point_on_segment(b, c, d))
}

fn orient2(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_on_segment(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> bool {
    point[0] >= start[0].min(end[0]) - POLYGON_INTERSECTION_EPSILON
        && point[0] <= start[0].max(end[0]) + POLYGON_INTERSECTION_EPSILON
        && point[1] >= start[1].min(end[1]) - POLYGON_INTERSECTION_EPSILON
        && point[1] <= start[1].max(end[1]) + POLYGON_INTERSECTION_EPSILON
}

fn polygon_plane_normal(points: &[[f64; 3]]) -> Result<[f64; 3], EngineError> {
    let origin = *points.first().ok_or_else(|| fail("empty polygon face"))?;
    for first in 1..points.len().saturating_sub(1) {
        let u = [
            points[first][0] - origin[0],
            points[first][1] - origin[1],
            points[first][2] - origin[2],
        ];
        if u.iter().any(|value| !value.is_finite()) {
            return Err(fail("polygon coordinate difference overflow"));
        }
        for point in points.iter().skip(first + 1) {
            let v = [
                point[0] - origin[0],
                point[1] - origin[1],
                point[2] - origin[2],
            ];
            if v.iter().any(|value| !value.is_finite()) {
                return Err(fail("polygon coordinate difference overflow"));
            }
            let scale = u
                .iter()
                .chain(&v)
                .map(|value| value.abs())
                .fold(0.0, f64::max);
            if scale == 0.0 {
                continue;
            }
            let u = u.map(|value| value / scale);
            let v = v.map(|value| value / scale);
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let length = cross[0].hypot(cross[1]).hypot(cross[2]);
            if length.is_finite() && length > 0.0 {
                return Ok([cross[0] / length, cross[1] / length, cross[2] / length]);
            }
        }
    }
    Err(fail("degenerate polygon face"))
}

fn transform_part(
    part: &mut ModelMeshPart,
    transform: MeshTransform,
    topology_only: bool,
) -> Result<(), EngineError> {
    if transform.is_identity() {
        return Ok(());
    }
    for position in &mut part.positions_mm {
        *position = transform.apply(*position);
        if position.iter().any(|value| !value.is_finite()) {
            return Err(fail("transformed coordinate overflow"));
        }
    }
    if transform.reverses_winding {
        for triangle in &mut part.triangles {
            triangle.swap(1, 2);
        }
    }
    part.flat_normals.clear();
    for triangle in &part.triangles {
        part.flat_normals.push(if topology_only {
            [0.0, 0.0, 1.0]
        } else {
            normal(
                part.positions_mm[triangle[0] as usize],
                part.positions_mm[triangle[1] as usize],
                part.positions_mm[triangle[2] as usize],
            )?
        });
    }
    Ok(())
}
fn normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Result<[f64; 3], EngineError> {
    let mut u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let mut v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let scale = u
        .iter()
        .chain(v.iter())
        .map(|x| x.abs())
        .fold(0.0_f64, f64::max);
    if !scale.is_finite() || scale == 0.0 {
        return Err(fail("degenerate or overflowing triangle"));
    }
    for i in 0..3 {
        u[i] /= scale;
        v[i] /= scale;
    }
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = cross[0].hypot(cross[1]).hypot(cross[2]);
    if !length.is_finite() || length == 0.0 {
        return Err(fail("degenerate triangle"));
    }
    Ok([cross[0] / length, cross[1] / length, cross[2] / length])
}

#[cfg(test)]
mod derivation_tests {
    use super::*;

    const TRIANGLE: &str = "Shape { geometry IndexedFaceSet { coord Coordinate { point [0 0 0, 1 0 0, 0 1 0] } coordIndex [0, 1, 2, -1] solid TRUE } }";

    #[test]
    fn polygon_work_budget_is_shared_across_indexed_face_sets() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ];
        let face = vec![vec![0, 1, 2, 3]];
        let mut remaining_work = 63;

        let first = triangulate_indexed_faces(&face, &positions, 2, &mut remaining_work).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(remaining_work, 31);

        // A second individually affordable face set is rejected because both
        // sets share the parser's one input-wide quadratic-work budget.
        assert!(triangulate_indexed_faces(&face, &positions, 2, &mut remaining_work).is_err());
    }

    fn sample(invalid: &str, uses: &str) -> Vec<u8> {
        format!("#VRML V2.0 utf8\nShape {{ appearance Appearance {{ material DEF VALID Material {{ diffuseColor 0.2 0.3 0.4 }} }} geometry IndexedFaceSet {{ coord Coordinate {{ point [0 0 0, 1 0 0, 0 1 0] }} coordIndex [0, 1, 2, -1] solid TRUE }} }}\nShape {{ appearance Appearance {{ material DEF UNUSED Material {{ diffuseColor 0.2 0.3 0.4 }} }} }}\nShape {{ appearance Appearance {{ material DEF BAD Material {{ ambientIntensity {invalid} }} }} }}\n{uses}\n{TRIANGLE}\n").into_bytes()
    }
    #[test]
    fn removes_only_unused_out_of_range_palette_preserves_valid_palette_and_geometry() {
        let input = sample(
            "2.0",
            "Shape { appearance Appearance { material USE VALID } }",
        );
        let (output, report) =
            derive_without_unused_invalid_materials(&input, ModelUnits::Millimetres).unwrap();
        let text = std::str::from_utf8(&output).unwrap();
        assert!(text.contains("DEF VALID"));
        assert!(text.contains("DEF UNUSED"));
        assert!(!text.contains("DEF BAD"));
        assert_eq!(report["removedPaletteShapes"].as_array().unwrap().len(), 1);
        assert_eq!(report["geometryModified"], false);
        assert_eq!(report["derivedMeshParts"], 2);
    }
    #[test]
    fn used_invalid_material_and_unsupported_material_node_refuse() {
        let used = sample(
            "2.0",
            "Shape { appearance Appearance { material USE BAD } }",
        );
        assert!(derive_without_unused_invalid_materials(&used, ModelUnits::Millimetres).is_err());
        let unsupported=b"#VRML V2.0 utf8\nShape { appearance Appearance { texture ImageTexture { url \"x\" } } }";
        assert!(
            derive_without_unused_invalid_materials(unsupported, ModelUnits::Millimetres).is_err()
        );
    }
}
