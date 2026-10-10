//! Bounded, original VRML97 reader for flat Shape/triangle IndexedFaceSet models.
//! No scene transforms, external resources, STEP, textures, or general VRML evaluation.
//! Source coordinates require an explicit unit declaration; output is local millimetres.
use crate::EngineError;
use std::collections::{BTreeMap, BTreeSet};

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
/// Vertex and triangle budgets accumulate across the entire input.
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

/// Parse only the declared flat, triangle-only VRML97 profile. Any unsupported
/// field/node is an error, including Transform, Inline, normal and texture data.
/// This API performs no filesystem or network access.
pub fn parse_vrml_mesh(
    input: &[u8],
    units: ModelUnits,
    limits: MeshLimits,
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
        vertices: 0,
        triangles: 0,
    };
    let mut parts = Vec::new();
    let mut shape_count = 0;
    while parser.cursor < parser.tokens.len() {
        if shape_count >= limits.max_shapes {
            return Err(fail("shape budget exceeded"));
        }
        parser.expect("Shape")?;
        if let Some(part) = parser.shape(shape_count)? {
            parts.push(part);
        }
        shape_count += 1;
    }
    if parts.is_empty() {
        return Err(fail("no nonempty triangle geometry"));
    }
    let diagnostics = vec![
        "Triangle-only flat VRML profile; local model geometry is not PCB placement or a complete CAD solid.".into(),
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
struct Parser<'a> {
    tokens: Vec<&'a str>,
    cursor: usize,
    units: ModelUnits,
    limits: MeshLimits,
    materials: BTreeMap<String, MeshMaterial>,
    vertices: usize,
    triangles: usize,
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
            if name.is_empty()
                || !name.as_bytes()[0].is_ascii_alphabetic()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            {
                return Err(fail("unsupported material DEF identifier"));
            }
            if self.materials.contains_key(name) {
                return Err(fail("duplicate material DEF"));
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
    fn indices(&mut self) -> Result<Vec<[u32; 3]>, EngineError> {
        self.expect("[")?;
        let mut triangles = Vec::new();
        let mut face = Vec::new();
        while !self.closing("]") {
            let token = self.take()?;
            let index: i64 = token
                .parse()
                .map_err(|_| fail("coordIndex must contain integers"))?;
            if index == -1 {
                if face.len() != 3 {
                    return Err(fail("only triangular faces supported"));
                }
                if self.triangles >= self.limits.max_triangles {
                    return Err(fail("triangle budget exceeded"));
                }
                triangles.push([face[0], face[1], face[2]]);
                self.triangles += 1;
                face.clear();
            } else {
                if index < 0 || index > u32::MAX as i64 {
                    return Err(fail("invalid coordinate index"));
                }
                if face.len() >= 3 {
                    return Err(fail("only triangular faces supported"));
                }
                face.push(index as u32);
            }
        }
        if !face.is_empty() {
            return Err(fail("face missing -1 terminator"));
        }
        Ok(triangles)
    }
    fn geometry(&mut self, ordinal: usize) -> Result<ModelMeshPart, EngineError> {
        self.expect("IndexedFaceSet")?;
        self.expect("{")?;
        let mut seen = BTreeSet::new();
        let mut positions = None;
        let mut triangles = None;
        let mut crease_angle = 0.0;
        let mut ccw = true;
        let mut solid = true;
        while !self.closing("}") {
            match self.field(&mut seen)? {
                "coord" => positions = Some(self.coordinates()?),
                "coordIndex" => triangles = Some(self.indices()?),
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
        let mut triangles = triangles.ok_or_else(|| fail("missing coordIndex"))?;
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
            normals.push(normal(
                positions[triangle[0] as usize],
                positions[triangle[1] as usize],
                positions[triangle[2] as usize],
            )?);
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
