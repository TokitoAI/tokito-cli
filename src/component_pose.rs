//! KiCad STEP-export component point placement, expressed in the Tokito GLB frame.
//!
//! This pure-Rust module reproduces the placement contract observed in the
//! pinned KiCad source; it does not parse STEP/WRL assets or invoke KiCad.
//! Model-space input coordinates and every placement distance are millimetres.
//! The caller must supply stackup-derived outer-copper Z planes explicitly.
use crate::EngineError;
use crate::model_mesh::{MeshMaterial, ModelMesh};

/// KiCad CLI STEP exporter clearance between a model origin and copper plane.
pub const KICAD_STEP_BOARD_OFFSET_MM: f64 = 0.05;
const MM_TO_M: f64 = 0.001;
const MAX_PLACED_VERTICES: usize = 200_000;
const MAX_PLACED_TRIANGLES: usize = 200_000;

/// One VRML shape after placement in the Tokito GLB coordinate frame.
#[derive(Clone, Debug)]
pub struct PlacedModelMeshPart {
    pub source_shape_ordinal: usize,
    /// Positions are metres in the right-handed GLB frame (X right, Y up, Z board-depth).
    pub positions_m: Vec<[f64; 3]>,
    /// Indices preserve outward orientation after the component transform.
    pub triangles: Vec<[u32; 3]>,
    /// Recomputed flat unit normals, one per placed triangle.
    pub flat_normals: Vec<[f64; 3]>,
    /// Per-corner unit normals after the source/model/board transforms.
    pub corner_normals: Vec<[[f64; 3]; 3]>,
    pub material: Option<MeshMaterial>,
    pub source_crease_angle_radians: f64,
    pub solid: bool,
}

/// Parsed model geometry transformed with explicit source placement data.
/// This is a placement result only; it does not claim STEP/VRML equivalence or
/// emit a GLB node on its own.
#[derive(Clone, Debug)]
pub struct PlacedModelMesh {
    pub parts: Vec<PlacedModelMeshPart>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardSide {
    Front,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelTransformMm {
    /// Per-axis model scale. Values must be finite and non-zero.
    pub scale: [f64; 3],
    /// KiCad model rotation in degrees, in X/Y/Z order.
    pub rotation_deg: [f64; 3],
    /// Normalized model offset in millimetres; convert legacy at fields first.
    pub offset_mm: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KicadStepPlacement {
    /// Saved PCB coordinates; source Y increases down the board.
    pub board_position_mm: [f64; 2],
    /// The board origin used by both this placement and the board-shell export.
    pub board_origin_mm: [f64; 2],
    /// Footprint orientation in degrees as stored by the PCB importer.
    pub footprint_rotation_deg: f64,
    pub side: BoardSide,
    /// Front outer-copper surface in the KiCad STEP frame, in millimetres.
    pub front_copper_surface_z_mm: f64,
    /// Back outer-copper surface in the KiCad STEP frame, normally negative.
    pub back_copper_surface_z_mm: f64,
    pub model: ModelTransformMm,
}

/// Transform one model-space point (already converted to millimetres) into the
/// Tokito GLB frame: metres, X right, Y up, and Z along the board plane.
///
/// Composition follows the pinned KiCad STEP exporter behavior:
/// model scale; model rotations -X, -Y, -Z; model offset and board clearance;
/// for the back, a 180-degree X rotation; footprint Z rotation; then footprint
/// position (x, -y). The explicit copper planes avoid assuming that the board
/// slab surfaces equal KiCad's outer-copper surfaces.
pub fn place_model_point_in_glb_m(
    model_point_mm: [f64; 3],
    placement: KicadStepPlacement,
) -> Result<[f64; 3], EngineError> {
    validate_placement(placement)?;
    if model_point_mm.iter().any(|value| !value.is_finite()) {
        return Err(fail("model point contains a non-finite value"));
    }
    let mut point = [
        model_point_mm[0] * placement.model.scale[0],
        model_point_mm[1] * placement.model.scale[1],
        model_point_mm[2] * placement.model.scale[2],
    ];

    // Apply the source-defined signed rotations as explicit point operations.
    point = rotate_x(point, -placement.model.rotation_deg[0]);
    point = rotate_y(point, -placement.model.rotation_deg[1]);
    point = rotate_z(point, -placement.model.rotation_deg[2]);

    point[0] += placement.model.offset_mm[0];
    point[1] += placement.model.offset_mm[1];
    point[2] += placement.model.offset_mm[2]
        + KICAD_STEP_BOARD_OFFSET_MM
        + match placement.side {
            BoardSide::Front => placement.front_copper_surface_z_mm,
            BoardSide::Back => -placement.back_copper_surface_z_mm,
        };

    if placement.side == BoardSide::Back {
        point = rotate_x(point, 180.0);
    }
    point = rotate_z(point, placement.footprint_rotation_deg);

    // KiCad's STEP placement uses (x, -y) in its board plane. The current
    // Tokito shell uses (x-origin_x, -[y-origin_y]) in GLB X/Z.
    let step_x_mm = placement.board_position_mm[0] - placement.board_origin_mm[0] + point[0];
    let step_y_mm = -(placement.board_position_mm[1] - placement.board_origin_mm[1]) + point[1];
    let glb = [step_x_mm * MM_TO_M, point[2] * MM_TO_M, step_y_mm * MM_TO_M];
    if glb.iter().any(|value| !value.is_finite()) {
        return Err(fail("placement result overflowed"));
    }
    Ok(glb)
}

/// Transform a bounded parsed triangle mesh into GLB space. Normals are
/// recomputed from transformed triangles. The KiCad STEP basis maps to the
/// Tokito GLB basis as [x, z, y], a reflection that aligns component placement
/// with the board-shell's saved Y-down coordinates. Model-scale parity
/// composes with this fixed reflection when correcting triangle winding.
pub fn place_model_mesh_in_glb_m(
    mesh: &ModelMesh,
    placement: KicadStepPlacement,
) -> Result<PlacedModelMesh, EngineError> {
    let reverse_winding = validate_placement(placement)?;
    if mesh.parts.is_empty() {
        return Err(fail("model mesh contains no parts"));
    }

    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    let mut parts = Vec::with_capacity(mesh.parts.len());
    for part in &mesh.parts {
        total_vertices = total_vertices
            .checked_add(part.positions_mm.len())
            .ok_or_else(|| fail("model vertex count overflow"))?;
        total_triangles = total_triangles
            .checked_add(part.triangles.len())
            .ok_or_else(|| fail("model triangle count overflow"))?;
        if total_vertices > MAX_PLACED_VERTICES || total_triangles > MAX_PLACED_TRIANGLES {
            return Err(fail("placed model exceeds vertex or triangle budget"));
        }
        if part.positions_mm.is_empty()
            || part.triangles.is_empty()
            || part.flat_normals.len() != part.triangles.len()
            || part.corner_normals.len() != part.triangles.len()
        {
            return Err(fail("model part has inconsistent triangle geometry"));
        }

        let positions_m = part
            .positions_mm
            .iter()
            .map(|point| place_model_point_in_glb_m(*point, placement))
            .collect::<Result<Vec<_>, _>>()?;
        let mut triangles = Vec::with_capacity(part.triangles.len());
        let mut flat_normals = Vec::with_capacity(part.triangles.len());
        let mut corner_normals = Vec::with_capacity(part.triangles.len());
        for (triangle_index, source_triangle) in part.triangles.iter().enumerate() {
            let mut triangle = *source_triangle;
            let mut normals = part.corner_normals[triangle_index];
            if triangle
                .iter()
                .any(|index| *index as usize >= positions_m.len())
            {
                return Err(fail("model triangle index is out of range"));
            }
            if reverse_winding {
                triangle.swap(1, 2);
                normals.swap(1, 2);
            }
            for normal in &mut normals {
                *normal = place_model_normal_in_glb(*normal, placement)?;
            }
            let a = positions_m[triangle[0] as usize];
            let b = positions_m[triangle[1] as usize];
            let c = positions_m[triangle[2] as usize];
            flat_normals.push(triangle_normal(a, b, c)?);
            triangles.push(triangle);
            corner_normals.push(normals);
        }

        parts.push(PlacedModelMeshPart {
            source_shape_ordinal: part.source_shape_ordinal,
            positions_m,
            triangles,
            flat_normals,
            corner_normals,
            material: part.material.clone(),
            source_crease_angle_radians: part.source_crease_angle_radians,
            solid: part.solid,
        });
    }

    Ok(PlacedModelMesh {
        parts,
        diagnostics: mesh.diagnostics.clone(),
    })
}

/// Transform a local VRML normal by the inverse transpose of the KiCad-to-GLB
/// linear map. Translations do not apply to normals; the basis reflection is
/// retained while triangle winding is corrected separately.
fn place_model_normal_in_glb(
    normal: [f64; 3],
    placement: KicadStepPlacement,
) -> Result<[f64; 3], EngineError> {
    if normal.iter().any(|value| !value.is_finite()) {
        return Err(fail("source model normal contains a non-finite value"));
    }
    let mut vector = [
        normal[0] / placement.model.scale[0],
        normal[1] / placement.model.scale[1],
        normal[2] / placement.model.scale[2],
    ];
    vector = rotate_x(vector, -placement.model.rotation_deg[0]);
    vector = rotate_y(vector, -placement.model.rotation_deg[1]);
    vector = rotate_z(vector, -placement.model.rotation_deg[2]);
    if placement.side == BoardSide::Back {
        vector = rotate_x(vector, 180.0);
    }
    vector = rotate_z(vector, placement.footprint_rotation_deg);
    // STEP/model XYZ -> Tokito GLB XYZ is the same Y/Z swap as for points.
    normalize(vector[0], vector[2], vector[1])
}

fn normalize(x: f64, y: f64, z: f64) -> Result<[f64; 3], EngineError> {
    let vector = [x, y, z];
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(fail("transformed model normal overflowed"));
    }
    let scale = vector.iter().map(|value| value.abs()).fold(0.0, f64::max);
    if scale == 0.0 {
        return Err(fail("transformed model normal is degenerate"));
    }
    let scaled = vector.map(|value| value / scale);
    let length = scaled[0].hypot(scaled[1]).hypot(scaled[2]);
    if !length.is_finite() || length == 0.0 {
        return Err(fail("transformed model normal is degenerate"));
    }
    Ok(scaled.map(|value| value / length))
}

/// Validate the full placement and report whether its linear transform is a
/// reflection. The fixed STEP-to-GLB basis has negative parity; model scale
/// parity composes with it. Rotations preserve parity.
fn validate_placement(placement: KicadStepPlacement) -> Result<bool, EngineError> {
    let scalars = [
        placement.board_position_mm[0],
        placement.board_position_mm[1],
        placement.board_origin_mm[0],
        placement.board_origin_mm[1],
        placement.footprint_rotation_deg,
        placement.front_copper_surface_z_mm,
        placement.back_copper_surface_z_mm,
        placement.model.scale[0],
        placement.model.scale[1],
        placement.model.scale[2],
        placement.model.rotation_deg[0],
        placement.model.rotation_deg[1],
        placement.model.rotation_deg[2],
        placement.model.offset_mm[0],
        placement.model.offset_mm[1],
        placement.model.offset_mm[2],
    ];
    if scalars.iter().any(|value| !value.is_finite()) {
        return Err(fail("placement contains a non-finite value"));
    }
    if placement.model.scale.contains(&0.0) {
        return Err(fail("model scale contains a zero axis"));
    }
    if placement.front_copper_surface_z_mm <= placement.back_copper_surface_z_mm {
        return Err(fail(
            "front copper surface must be above the back copper surface",
        ));
    }
    let odd_negative_scale = placement
        .model
        .scale
        .iter()
        .filter(|axis| axis.is_sign_negative())
        .count()
        % 2
        == 1;
    Ok(!odd_negative_scale)
}

fn triangle_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Result<[f64; 3], EngineError> {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    if cross.iter().any(|value| !value.is_finite()) {
        return Err(fail("transformed triangle normal overflowed"));
    }
    let length = cross[0].hypot(cross[1]).hypot(cross[2]);
    if !length.is_finite() || length <= 0.0 {
        return Err(fail("transformed model contains a degenerate triangle"));
    }
    Ok([cross[0] / length, cross[1] / length, cross[2] / length])
}

fn rotate_x(point: [f64; 3], degrees: f64) -> [f64; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        point[0],
        cos * point[1] - sin * point[2],
        sin * point[1] + cos * point[2],
    ]
}

fn rotate_y(point: [f64; 3], degrees: f64) -> [f64; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        cos * point[0] + sin * point[2],
        point[1],
        -sin * point[0] + cos * point[2],
    ]
}

fn rotate_z(point: [f64; 3], degrees: f64) -> [f64; 3] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [
        cos * point[0] - sin * point[1],
        sin * point[0] + cos * point[1],
        point[2],
    ]
}

fn fail(message: &'static str) -> EngineError {
    EngineError::new(format!("component placement: {message}"))
}
