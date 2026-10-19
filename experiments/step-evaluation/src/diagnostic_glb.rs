//! Diagnostic GLB writer for reader-derived STEP occurrence meshes.
//!
//! This emits a viewable, selectable-by-occurrence candidate only. It does not
//! qualify STEP surfaces, units, component identity, PCB placement, or solid
//! completeness and is deliberately not wired into Tokito CLI production export.

use gltf::binary::{Glb, Header};
use serde_json::{json, Value};
use sha2::Digest;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_GLTF_BYTES: usize = 128 * 1024 * 1024;
const MAX_GLTF_JSON_BYTES: usize = 64 * 1024 * 1024;
const MAX_TRIANGLE_CORNERS: usize = 3_000_000;
const MAX_OCCURRENCE_PATH_ENTRIES: usize = 1_000_000;
const MAX_PRIMITIVES: usize = 100_000;
const MAX_MATERIALS: usize = 4_096;
const MAX_POSITION_ERROR_METRES: f64 = 1e-6;
const DEFAULT_COLOR: [f32; 4] = [0.56, 0.58, 0.62, 1.0];
pub const MODEL14_SOURCE_SHA256: &str =
    "78db318947fa4d67e3d1f5ad9e7fbe3c44c9b0ffca621a8b8a0bb6dd93282b2b";
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct PrimitiveData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    indices: Vec<u32>,
    triangle_ordinals: Vec<usize>,
    source_face_ids: Vec<u64>,
    degenerate_triangle_ordinals: Vec<usize>,
    minimum: [f32; 3],
    maximum: [f32; 3],
    initialized: bool,
}

#[derive(Debug)]
struct Output {
    bytes: Vec<u8>,
    primitive_count: usize,
    triangle_count: usize,
    degenerate_triangle_count: usize,
    occurrence_count: usize,
    geometryless_occurrence_count: usize,
}

fn err(message: &str) -> String {
    message.to_owned()
}

fn bounded_json_length(value: &Value) -> Result<usize, String> {
    struct Counter(usize);
    impl io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|length| *length <= MAX_GLTF_JSON_BYTES)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "GLB JSON budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value)
        .map_err(|_| err("diagnostic_glb_json_size_limit"))?;
    Ok(counter.0)
}

fn create_temp_file(output_path: &Path) -> Result<(PathBuf, std::fs::File), String> {
    let parent = output_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    for _ in 0..16 {
        let ordinal = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".tokito-step-export-{}-{ordinal}.tmp",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(err("diagnostic_glb_temp_create_failed")),
        }
    }
    Err(err("diagnostic_glb_temp_name_exhausted"))
}

#[cfg(test)]
fn document_json(bytes: &[u8]) -> Value {
    let glb = Glb::from_slice(bytes).expect("GLB framing");
    serde_json::from_slice(&glb.json).expect("GLB JSON")
}

fn color_key(value: [f32; 4]) -> [u32; 4] {
    value.map(f32::to_bits)
}

fn parse_color(value: &Value) -> Result<[f32; 4], String> {
    let entries = value
        .as_array()
        .filter(|entries| entries.len() == 4)
        .ok_or_else(|| err("diagnostic_glb_invalid_color"))?;
    let mut color = [0.0; 4];
    for (index, entry) in entries.iter().enumerate() {
        let number = entry
            .as_f64()
            .filter(|number| number.is_finite() && (0.0..=1.0).contains(number))
            .ok_or_else(|| err("diagnostic_glb_invalid_color"))?;
        color[index] = number as f32;
        if !color[index].is_finite() {
            return Err(err("diagnostic_glb_invalid_color"));
        }
    }
    Ok(color)
}

fn vec3(value: &Value) -> Result<[f64; 3], String> {
    let entries = value
        .as_array()
        .filter(|entries| entries.len() == 3)
        .ok_or_else(|| err("diagnostic_glb_invalid_position"))?;
    let mut point = [0.0; 3];
    for (index, entry) in entries.iter().enumerate() {
        point[index] = entry
            .as_f64()
            .filter(|number| number.is_finite())
            .ok_or_else(|| err("diagnostic_glb_invalid_position"))?;
    }
    Ok(point)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn unit_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Result<([f64; 3], bool), String> {
    let n = cross(subtract(b, a), subtract(c, a));
    if n.iter().any(|value| !value.is_finite()) {
        return Err(err("diagnostic_glb_nonfinite_triangle_normal"));
    }
    let length = n.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !length.is_finite() {
        return Err(err("diagnostic_glb_nonfinite_triangle_normal"));
    }
    if length == 0.0 {
        return Ok(([0.0, 1.0, 0.0], true));
    }
    Ok((n.map(|value| value / length), false))
}

fn push_triangle(
    data: &mut PrimitiveData,
    points_mm: [[f64; 3]; 3],
    normal: [f64; 3],
    ordinal: usize,
    source_face_id: Option<u64>,
    degenerate: bool,
) -> Result<(), String> {
    if data.positions.len().saturating_add(3) > MAX_TRIANGLE_CORNERS {
        return Err(err("diagnostic_glb_triangle_corner_limit"));
    }
    let base =
        u32::try_from(data.positions.len()).map_err(|_| err("diagnostic_glb_index_overflow"))?;
    for point in points_mm {
        let metres = point.map(|value| value * 0.001);
        let emitted = metres.map(|value| value as f32);
        if metres.iter().zip(emitted).any(|(source, value)| {
            !source.is_finite()
                || !value.is_finite()
                || (value as f64 - source).abs() > MAX_POSITION_ERROR_METRES
        }) {
            return Err(err("diagnostic_glb_f32_position_error_limit"));
        }
        for (axis, value) in emitted.iter().copied().enumerate() {
            if !data.initialized {
                data.minimum[axis] = value;
                data.maximum[axis] = value;
            } else {
                data.minimum[axis] = data.minimum[axis].min(value);
                data.maximum[axis] = data.maximum[axis].max(value);
            }
        }
        data.positions.push(emitted);
        data.normals.push(normal.map(|value| value as f32));
        data.initialized = true;
    }
    data.indices.extend([base, base + 1, base + 2]);
    data.triangle_ordinals.push(ordinal);
    data.source_face_ids.push(source_face_id.unwrap_or(0));
    if degenerate {
        data.degenerate_triangle_ordinals.push(ordinal);
    }
    Ok(())
}

fn append_f32_array(binary: &mut Vec<u8>, values: &[[f32; 3]]) -> Result<(usize, usize), String> {
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let offset = binary.len();
    let length = values
        .len()
        .checked_mul(12)
        .ok_or_else(|| err("diagnostic_glb_buffer_overflow"))?;
    if binary
        .len()
        .checked_add(length)
        .filter(|length| *length <= MAX_GLTF_BYTES)
        .is_none()
    {
        return Err(err("diagnostic_glb_size_limit"));
    }
    for value in values {
        for axis in value {
            binary.extend_from_slice(&axis.to_le_bytes());
        }
    }
    Ok((offset, length))
}

fn append_u32_array(binary: &mut Vec<u8>, values: &[u32]) -> Result<(usize, usize), String> {
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let offset = binary.len();
    let length = values
        .len()
        .checked_mul(4)
        .ok_or_else(|| err("diagnostic_glb_buffer_overflow"))?;
    if binary
        .len()
        .checked_add(length)
        .filter(|length| *length <= MAX_GLTF_BYTES)
        .is_none()
    {
        return Err(err("diagnostic_glb_size_limit"));
    }
    for value in values {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    Ok((offset, length))
}

#[allow(clippy::too_many_arguments)] // One primitive appends three linked glTF payload tables.
fn add_primitive(
    data: PrimitiveData,
    color: [f32; 4],
    material_index: usize,
    shape_index: usize,
    source_item_id: u64,
    buffer_views: &mut Vec<Value>,
    accessors: &mut Vec<Value>,
    binary: &mut Vec<u8>,
) -> Result<Value, String> {
    if data.positions.is_empty()
        || data.positions.len() != data.normals.len()
        || data.positions.len() != data.indices.len()
        || data.triangle_ordinals.len() * 3 != data.indices.len()
    {
        return Err(err("diagnostic_glb_primitive_inventory_mismatch"));
    }
    let (position_offset, position_length) = append_f32_array(binary, &data.positions)?;
    let (normal_offset, normal_length) = append_f32_array(binary, &data.normals)?;
    let (index_offset, index_length) = append_u32_array(binary, &data.indices)?;
    let position_view = buffer_views.len();
    buffer_views.push(json!({"buffer":0,"byteOffset":position_offset,"byteLength":position_length,"target":34962}));
    let normal_view = buffer_views.len();
    buffer_views.push(
        json!({"buffer":0,"byteOffset":normal_offset,"byteLength":normal_length,"target":34962}),
    );
    let index_view = buffer_views.len();
    buffer_views.push(
        json!({"buffer":0,"byteOffset":index_offset,"byteLength":index_length,"target":34963}),
    );
    let position_accessor = accessors.len();
    accessors.push(json!({"bufferView":position_view,"byteOffset":0,"componentType":5126,"count":data.positions.len(),"type":"VEC3","min":data.minimum,"max":data.maximum}));
    let normal_accessor = accessors.len();
    accessors.push(json!({"bufferView":normal_view,"byteOffset":0,"componentType":5126,"count":data.normals.len(),"type":"VEC3"}));
    let index_accessor = accessors.len();
    accessors.push(json!({"bufferView":index_view,"byteOffset":0,"componentType":5125,"count":data.indices.len(),"type":"SCALAR","min":[0],"max":[data.indices.len().saturating_sub(1)]}));
    Ok(json!({
        "attributes":{"POSITION":position_accessor,"NORMAL":normal_accessor},
        "indices":index_accessor,
        "material":material_index,
        "mode":4,
        "extras":{"tokito":{"sourceShapeIndex":shape_index,"sourceRepresentationItemId":source_item_id,
            "sourceTriangleOrdinals":data.triangle_ordinals,"sourceTriangleFaceIds":data.source_face_ids,
            "degenerateTriangleOrdinals":data.degenerate_triangle_ordinals,
            "sourceTriangleFaceIdZeroMeansUnassigned":true,
            "materialRgba":color,"normalGeneration":"flat-per-triangle-from-reader-tessellation",
            "surfaceGeometryQualified":false}}
    }))
}

fn source_id(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn path_text(value: &Value) -> Result<(Vec<usize>, String), String> {
    let path = value
        .as_array()
        .filter(|path| !path.is_empty() && path.len() <= 64)
        .ok_or_else(|| err("diagnostic_glb_invalid_occurrence_path"))?
        .iter()
        .map(|entry| {
            entry
                .as_u64()
                .and_then(|number| usize::try_from(number).ok())
                .ok_or_else(|| err("diagnostic_glb_invalid_occurrence_path"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let text = path
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("/");
    Ok((path, text))
}

fn validate_pinned_repair_candidate(
    candidate: &Value,
    source_sha256: &str,
    repair: &Value,
) -> Result<(), String> {
    if candidate["sourceSha256"] != source_sha256
        || candidate["experimentalPlanarRepair"] != *repair
    {
        return Err(err("diagnostic_glb_repair_candidate_provenance_mismatch"));
    }
    let occurrences = candidate["occurrences"]
        .as_array()
        .ok_or_else(|| err("diagnostic_glb_repair_candidate_occurrences_missing"))?;
    let item_id = repair["itemId"]
        .as_u64()
        .ok_or_else(|| err("diagnostic_glb_invalid_experimental_planar_repair_context"))?;
    let face_id = repair["faceId"]
        .as_u64()
        .ok_or_else(|| err("diagnostic_glb_invalid_experimental_planar_repair_context"))?;
    let repaired_triangles = repair["newFaceTriangles"]
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| err("diagnostic_glb_invalid_experimental_planar_repair_context"))?;
    let mut target_shapes = occurrences
        .iter()
        .flat_map(|occurrence| occurrence["shapes"].as_array().into_iter().flatten())
        .filter(|shape| shape["sourceItemId"].as_u64() == Some(item_id));
    let shape = target_shapes
        .next()
        .ok_or_else(|| err("diagnostic_glb_repair_target_shape_missing"))?;
    if target_shapes.next().is_some() {
        return Err(err("diagnostic_glb_repair_target_shape_ambiguous"));
    }
    let triangles = shape["triangles"]
        .as_array()
        .ok_or_else(|| err("diagnostic_glb_repair_target_triangles_missing"))?;
    let face_ids = shape["sourceTriangleFaceIds"]
        .as_array()
        .filter(|ids| ids.len() == triangles.len())
        .ok_or_else(|| err("diagnostic_glb_repair_target_face_provenance_mismatch"))?;
    if face_ids
        .iter()
        .filter(|id| id.as_u64() == Some(face_id))
        .count()
        != repaired_triangles
    {
        return Err(err(
            "diagnostic_glb_repair_target_face_triangle_count_mismatch",
        ));
    }
    Ok(())
}

fn build(
    candidate: &Value,
    source_sha256: &str,
    experimental_planar_repair: Option<&Value>,
) -> Result<Output, String> {
    if candidate["schema"] != "tokito.step-positioned-candidate-mesh.v1"
        || candidate["lengthUnit"] != "mm"
        || candidate["geometryQualified"] != false
        || candidate["surfaceCompletenessVerified"] != false
        || candidate["pcbPlacementQualified"] != false
        || candidate["normalsGenerated"] != false
    {
        return Err(err(
            "diagnostic_glb_requires_unqualified_positioned_candidate_mesh_v1",
        ));
    }
    let model_units_to_mm = candidate["lengthScaleModelUnitsToMm"]
        .as_f64()
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .ok_or_else(|| err("diagnostic_glb_missing_independent_unit_scale"))?;
    let occurrences = candidate["occurrences"]
        .as_array()
        .filter(|occurrences| !occurrences.is_empty() && occurrences.len() <= 100_000)
        .ok_or_else(|| err("diagnostic_glb_occurrence_limit_or_missing"))?;
    let mut binary = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut materials = Vec::new();
    let mut material_indices = BTreeMap::<[u32; 4], usize>::new();
    let mut meshes = Vec::with_capacity(occurrences.len());
    let mut nodes = Vec::with_capacity(occurrences.len());
    let mut roots = Vec::new();
    let mut index_by_path = BTreeMap::<Vec<usize>, usize>::new();
    let mut geometryless_occurrence_count = 0usize;
    let mut primitive_count = 0usize;
    let mut triangle_count = 0usize;
    let mut degenerate_triangle_count = 0usize;
    let mut occurrence_path_entries = 0usize;
    let mut previous_path: Option<Vec<usize>> = None;

    for occurrence in occurrences {
        let (path, path_id) = path_text(&occurrence["occurrencePath"])?;
        occurrence_path_entries = occurrence_path_entries
            .checked_add(path.len())
            .filter(|count| *count <= MAX_OCCURRENCE_PATH_ENTRIES)
            .ok_or_else(|| err("diagnostic_glb_occurrence_path_budget"))?;
        if index_by_path.contains_key(&path)
            || previous_path.as_ref().is_some_and(|prior| prior >= &path)
        {
            return Err(err(
                "diagnostic_glb_occurrence_paths_not_unique_depth_first_order",
            ));
        }
        let shapes = occurrence["shapes"]
            .as_array()
            .ok_or_else(|| err("diagnostic_glb_missing_shapes"))?;
        let mut primitives = Vec::new();
        for (shape_index, shape) in shapes.iter().enumerate() {
            let vertices = shape["verticesMm"]
                .as_array()
                .ok_or_else(|| err("diagnostic_glb_missing_vertices"))?;
            let triangles = shape["triangles"]
                .as_array()
                .ok_or_else(|| err("diagnostic_glb_missing_triangles"))?;
            let face_ids = shape["sourceTriangleFaceIds"]
                .as_array()
                .ok_or_else(|| err("diagnostic_glb_missing_triangle_face_provenance"))?;
            let triangle_colors = shape["sourceTriangleRgba"]
                .as_array()
                .ok_or_else(|| err("diagnostic_glb_missing_triangle_color_provenance"))?;
            if !face_ids.is_empty() && face_ids.len() != triangles.len() {
                return Err(err("diagnostic_glb_triangle_face_provenance_mismatch"));
            }
            if !triangle_colors.is_empty() && triangle_colors.len() != triangles.len() {
                return Err(err("diagnostic_glb_triangle_color_provenance_mismatch"));
            }
            let shape_color = shape.get("sourceItemRgba").filter(|value| !value.is_null());
            let shape_color = shape_color
                .map(parse_color)
                .transpose()?
                .unwrap_or(DEFAULT_COLOR);
            let mut groups = BTreeMap::<[u32; 4], ([f32; 4], PrimitiveData)>::new();
            for (triangle_ordinal, triangle) in triangles.iter().enumerate() {
                if triangle_count.saturating_add(1).saturating_mul(3) > MAX_TRIANGLE_CORNERS {
                    return Err(err("diagnostic_glb_triangle_corner_limit"));
                }
                let indices = triangle
                    .as_array()
                    .filter(|indices| indices.len() == 3)
                    .ok_or_else(|| err("diagnostic_glb_invalid_triangle"))?;
                let mut points = [[0.0; 3]; 3];
                for (corner, index) in indices.iter().enumerate() {
                    let index = index
                        .as_u64()
                        .and_then(|index| usize::try_from(index).ok())
                        .filter(|index| *index < vertices.len())
                        .ok_or_else(|| err("diagnostic_glb_triangle_index_out_of_range"))?;
                    points[corner] = vec3(&vertices[index])?;
                }
                let (normal, degenerate) = unit_normal(points[0], points[1], points[2])?;
                let color =
                    if triangle_colors.is_empty() || triangle_colors[triangle_ordinal].is_null() {
                        shape_color
                    } else {
                        parse_color(&triangle_colors[triangle_ordinal])?
                    };
                let key = color_key(color);
                let group = groups
                    .entry(key)
                    .or_insert_with(|| (color, PrimitiveData::default()));
                let face_id = face_ids.get(triangle_ordinal).and_then(Value::as_u64);
                push_triangle(
                    &mut group.1,
                    points,
                    normal,
                    triangle_ordinal,
                    face_id,
                    degenerate,
                )?;
                degenerate_triangle_count += usize::from(degenerate);
                triangle_count = triangle_count
                    .checked_add(1)
                    .filter(|count| *count <= 2_000_000)
                    .ok_or_else(|| err("diagnostic_glb_triangle_limit"))?;
            }
            let source_item_id = source_id(shape, "sourceItemId")
                .ok_or_else(|| err("diagnostic_glb_missing_shape_source_item_id"))?;
            for (color, data) in groups.into_values() {
                if primitive_count >= MAX_PRIMITIVES {
                    return Err(err("diagnostic_glb_primitive_limit"));
                }
                let key = color_key(color);
                let material_index = if let Some(&index) = material_indices.get(&key) {
                    index
                } else {
                    if materials.len() >= MAX_MATERIALS {
                        return Err(err("diagnostic_glb_material_limit"));
                    }
                    let index = materials.len();
                    materials.push(json!({"name":format!("STEP candidate material {index}"),
                        "pbrMetallicRoughness":{"baseColorFactor":color,"metallicFactor":0.0,"roughnessFactor":0.78},
                        "doubleSided":true,"extras":{"tokito":{"sourceColorProjection":"STEP reader color observation; not physical material validation"}}}));
                    material_indices.insert(key, index);
                    index
                };
                primitives.push(add_primitive(
                    data,
                    color,
                    material_index,
                    shape_index,
                    source_item_id,
                    &mut buffer_views,
                    &mut accessors,
                    &mut binary,
                )?);
                primitive_count += 1;
            }
        }
        let name = occurrence["name"]
            .as_str()
            .or_else(|| occurrence["partName"].as_str())
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("STEP occurrence {path_id}"));
        let has_mesh = !primitives.is_empty();
        if !has_mesh {
            geometryless_occurrence_count += 1;
        }
        let mesh_index = if has_mesh {
            let index = meshes.len();
            meshes.push(json!({"name":name,"primitives":primitives}));
            Some(index)
        } else {
            None
        };
        let node_index = nodes.len();
        let mut node = json!({"name":name,"children":[],
            "extras":{"tokito":{"selectorContract":"step-occurrence-path-v1","selectorId":format!("step-occurrence:{path_id}"),
                "selectionScope":"step_occurrence","sourceOccurrencePath":path,"sourceOccurrenceId":occurrence.get("sourceOccurrenceId").cloned().unwrap_or(Value::Null),
                "sourcePlacedById":occurrence.get("sourcePlacedById").cloned().unwrap_or(Value::Null),
                "sourcePartDefinitionId":occurrence.get("sourcePartDefinitionId").cloned().unwrap_or(Value::Null),
                "sourceProductId":occurrence.get("sourceProductId").cloned().unwrap_or(Value::Null),
                "componentReference":null,"componentUuid":null,"componentSelectable":false,
                "occurrenceSelectable":has_mesh,"geometryQualified":false,"pcbPlacementQualified":false}}});
        if let Some(mesh_index) = mesh_index {
            node["mesh"] = json!(mesh_index);
        }
        nodes.push(node);
        if path.len() == 1 {
            roots.push(node_index);
        } else {
            let parent_path = path[..path.len() - 1].to_vec();
            let parent = *index_by_path
                .get(&parent_path)
                .ok_or_else(|| err("diagnostic_glb_parent_occurrence_missing"))?;
            let children = nodes[parent]["children"]
                .as_array_mut()
                .ok_or_else(|| err("diagnostic_glb_parent_child_inventory_invalid"))?;
            children.push(json!(node_index));
        }
        index_by_path.insert(path.clone(), node_index);
        previous_path = Some(path);
    }
    if roots.is_empty() || triangle_count == 0 || binary.is_empty() {
        return Err(err("diagnostic_glb_no_occurrence_triangles"));
    }
    let mut asset_tokito = json!({"format":"tokito.step-unqualified-occurrence-glb.v1","experimental":true,
        "sourceSha256":source_sha256,"sourceFormat":"STEP","sourceLengthUnit":"mm",
        "sourceModelUnitsToMmScale":model_units_to_mm,"unitsIndependentlyPreflighted":true,
        "sourceAxesPreserved":true,"sourceUpAxisResolved":false,"geometryQualified":false,
        "surfaceCompletenessVerified":false,"completeSolidVerified":false,"pcbPlacementQualified":false,
        "componentReferenceMappingAvailable":false,"componentUuidMappingAvailable":false,"occurrenceColorsApplied":false,
        "selectionScope":"STEP assembly occurrence only","normalGeneration":"flat per triangle from reader tessellation; degenerate triangles use +Y display fallback",
        "degenerateTriangleCount":degenerate_triangle_count,
        "materialSemantics":"source STEP reader color observations projected to neutral PBR; physical materials unverified",
        "admission":{"admitted":false,"productionExport":false,"reason":"diagnostic candidate only; reader-derived surface tessellation, source-face completeness, exact solid geometry, and PCB component placement are unverified"}});
    if let Some(repair) = experimental_planar_repair {
        if repair["schema"] != "tokito.step-planar-repair.v1"
            || repair["faceId"] != 51_071
            || repair["representationId"] != 58_073
            || repair["itemId"] != 46_427
            || repair["oldFaceTriangles"] != 146
            || repair["newFaceTriangles"] != 146
            || repair["reusedBoundaryVertices"] != 148
            || repair["callerToleranceSourceUnits"] != 0.000_001
            || repair["sourceToModelLengthScale"] != 1.0
            || repair["maximumBoundaryMatchDistanceModelUnits"] != 0.0
            || repair["geometryQualified"] != false
            || repair["pcbPlacementQualified"] != false
            || repair["admitted"] != false
            || repair["productionExport"] != false
            || repair["positionsChanged"] != false
            || repair["occurrenceTransformsChanged"] != false
            || repair["unrelatedTrianglesChanged"] != false
        {
            return Err(err(
                "diagnostic_glb_invalid_experimental_planar_repair_context",
            ));
        }
        validate_pinned_repair_candidate(candidate, source_sha256, repair)?;
        asset_tokito["experimentalPlanarRepair"] = repair.clone();
    }
    let document = json!({
        "asset":{"version":"2.0","generator":"Tokito STEP experimental diagnostic exporter",
            "extras":{"tokito":asset_tokito}},
        "scene":0,"scenes":[{"name":"Unqualified STEP occurrence candidate","nodes":roots}],
        "nodes":nodes,"meshes":meshes,"materials":materials,
        "buffers":[{"byteLength":binary.len()}],"bufferViews":buffer_views,"accessors":accessors
    });
    let json_length = bounded_json_length(&document)?;
    let json_bytes = serde_json::to_vec(&document)
        .map_err(|_| err("diagnostic_glb_json_serialization_failed"))?;
    if json_bytes.len() != json_length {
        return Err(err("diagnostic_glb_json_length_changed"));
    }
    let glb_length = 12usize
        .checked_add(8)
        .and_then(|n| n.checked_add((json_bytes.len() + 3) & !3))
        .and_then(|n| {
            n.checked_add(if binary.is_empty() {
                0
            } else {
                8 + ((binary.len() + 3) & !3)
            })
        })
        .filter(|length| *length <= MAX_GLTF_BYTES)
        .ok_or_else(|| err("diagnostic_glb_size_limit"))?;
    let glb = Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: glb_length as u32,
        },
        json: Cow::Owned(json_bytes),
        bin: Some(Cow::Owned(binary)),
    };
    let bytes = glb
        .to_vec()
        .map_err(|_| err("diagnostic_glb_encoding_failed"))?;
    if bytes.len() != glb_length {
        return Err(err("diagnostic_glb_encoded_length_mismatch"));
    }
    gltf::Gltf::from_slice(&bytes).map_err(|_| err("diagnostic_glb_readback_validation_failed"))?;
    Ok(Output {
        bytes,
        primitive_count,
        triangle_count,
        degenerate_triangle_count,
        occurrence_count: occurrences.len(),
        geometryless_occurrence_count,
    })
}

pub fn write(
    candidate: &Value,
    source_sha256: &str,
    output_path: &std::path::Path,
) -> Result<Value, String> {
    if source_sha256.len() != 64 || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(err("diagnostic_glb_invalid_source_digest"));
    }
    if output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("glb"))
    {
        return Err(err("diagnostic_glb_output_extension_must_be_glb"));
    }
    write_inner(candidate, source_sha256, output_path, None)
}

/// Write one reviewed research repair context beside the occurrence GLB.
/// This entry point intentionally accepts only the pinned model-14 repair.
pub fn write_repaired_candidate(
    candidate: &Value,
    source_sha256: &str,
    output_path: &Path,
    repair: &Value,
) -> Result<Value, String> {
    if source_sha256 != MODEL14_SOURCE_SHA256 {
        return Err(err(
            "diagnostic_glb_repaired_candidate_requires_pinned_model14_source",
        ));
    }
    write_inner(candidate, source_sha256, output_path, Some(repair))
}

fn write_inner(
    candidate: &Value,
    source_sha256: &str,
    output_path: &Path,
    experimental_planar_repair: Option<&Value>,
) -> Result<Value, String> {
    if source_sha256.len() != 64 || !source_sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(err("diagnostic_glb_invalid_source_digest"));
    }
    if output_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("glb"))
    {
        return Err(err("diagnostic_glb_output_extension_must_be_glb"));
    }
    let output = build(candidate, source_sha256, experimental_planar_repair)?;
    let bytes = output.bytes;
    let (temporary_path, mut file) = create_temp_file(output_path)?;
    use std::io::Write;
    let write_result = file.write_all(&bytes).and_then(|()| file.sync_all());
    drop(file);
    if write_result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(err("diagnostic_glb_output_write_failed"));
    }
    if let Err(error) = std::fs::hard_link(&temporary_path, output_path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(if error.kind() == io::ErrorKind::AlreadyExists {
            err("diagnostic_glb_output_must_be_new")
        } else {
            err("diagnostic_glb_output_publish_failed")
        });
    }
    let temporary_file_cleanup_completed = std::fs::remove_file(&temporary_path).is_ok();
    Ok(
        json!({"schema":"tokito.step-diagnostic-glb-export.v1","outputFileName":output_path.file_name().and_then(|name|name.to_str()).unwrap_or("candidate.glb"),
        "outputSha256":format!("{:x}",sha2::Sha256::digest(&bytes)),"outputBytes":bytes.len(),
        "occurrenceCount":output.occurrence_count,"geometrylessOccurrenceCount":output.geometryless_occurrence_count,
        "primitiveCount":output.primitive_count,"triangleCount":output.triangle_count,
        "degenerateTriangleCount":output.degenerate_triangle_count,"temporaryFileCleanupCompleted":temporary_file_cleanup_completed,
        "selectorScope":"step_occurrence","componentReferenceMappingAvailable":false,"componentUuidMappingAvailable":false,
        "geometryQualified":false,"pcbPlacementQualified":false,"admitted":false,"productionExport":false,
        "normalGeneration":"flat per triangle; +Y fallback only for explicitly reported zero-area source triangles","experimentalPlanarRepairIncluded":experimental_planar_repair.is_some(),"glbReadbackValidated":true}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> Value {
        json!({"schema":"tokito.step-positioned-candidate-mesh.v1","lengthUnit":"mm","lengthScaleModelUnitsToMm":1.0,
        "geometryQualified":false,"surfaceCompletenessVerified":false,"pcbPlacementQualified":false,"normalsGenerated":false,
        "occurrences":[
            {"occurrencePath":[0],"sourceOccurrenceId":10,"partName":"assembly","shapes":[],"sourcePartDefinitionId":1},
            {"occurrencePath":[0,0],"sourceOccurrenceId":11,"partName":"QFN28","shapes":[
                {"sourceItemId":42,"verticesMm":[[0,0,0],[1,0,0],[0,1,0],[0,0,1]],"triangles":[[0,1,2],[0,3,1]],"sourceTriangleFaceIds":[101,102],"sourceTriangleRgba":[[0.8,0.2,0.1,1.0],[0.1,0.2,0.8,1.0]],"sourceItemRgba":null}
            ],"sourcePartDefinitionId":2}
        ]})
    }

    fn model14_repair() -> Value {
        json!({"schema":"tokito.step-planar-repair.v1","faceId":51071,
            "representationId":58073,"itemId":46427,"oldFaceTriangles":146,
            "newFaceTriangles":146,"reusedBoundaryVertices":148,
            "callerToleranceSourceUnits":0.000001,"sourceToModelLengthScale":1.0,
            "maximumBoundaryMatchDistanceModelUnits":0.0,
            "positionsChanged":false,"occurrenceTransformsChanged":false,
            "unrelatedTrianglesChanged":false,"geometryQualified":false,
            "pcbPlacementQualified":false,"admitted":false,"productionExport":false})
    }

    fn model14_repaired_candidate() -> (Value, Value) {
        let repair = model14_repair();
        let mut candidate = candidate();
        candidate["occurrences"][1]["shapes"][0]["sourceItemId"] = json!(46_427);
        candidate["occurrences"][1]["shapes"][0]["triangles"] = json!(vec![[0, 1, 2]; 146]);
        candidate["occurrences"][1]["shapes"][0]["sourceTriangleFaceIds"] =
            json!(vec![51_071; 146]);
        candidate["occurrences"][1]["shapes"][0]["sourceTriangleRgba"] =
            json!(vec![Value::Null; 146]);
        candidate["sourceSha256"] = json!(MODEL14_SOURCE_SHA256);
        candidate["experimentalPlanarRepair"] = repair.clone();
        (candidate, repair)
    }

    #[test]
    fn diagnostic_glb_preserves_selectable_occurrences_and_refuses_component_claims() {
        let output = build(&candidate(), &"a".repeat(64), None).unwrap();
        let document = document_json(&output.bytes);
        assert_eq!(document["scene"], 0);
        assert_eq!(document["scenes"][0]["nodes"], json!([0]));
        assert_eq!(document["nodes"][0]["children"], json!([1]));
        let child = &document["nodes"][1]["extras"]["tokito"];
        assert_eq!(child["selectorId"], "step-occurrence:0/0");
        assert_eq!(child["componentSelectable"], false);
        assert_eq!(
            document["meshes"][0]["primitives"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            document["meshes"][0]["primitives"][0]["extras"]["tokito"]["sourceTriangleFaceIds"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(output.triangle_count, 2);
        assert_eq!(output.geometryless_occurrence_count, 1);
    }

    #[test]
    fn diagnostic_glb_rejects_bad_indices_and_wrong_qualification_contract() {
        let mut bad_index = candidate();
        bad_index["occurrences"][1]["shapes"][0]["triangles"][0] = json!([0, 1, 99]);
        assert_eq!(
            build(&bad_index, &"a".repeat(64), None).unwrap_err(),
            "diagnostic_glb_triangle_index_out_of_range"
        );
        let mut qualified = candidate();
        qualified["geometryQualified"] = json!(true);
        assert_eq!(
            build(&qualified, &"a".repeat(64), None).unwrap_err(),
            "diagnostic_glb_requires_unqualified_positioned_candidate_mesh_v1"
        );
    }

    #[test]
    fn diagnostic_glb_requires_shape_identity_provenance() {
        let mut candidate = candidate();
        candidate["occurrences"][1]["shapes"][0]
            .as_object_mut()
            .unwrap()
            .remove("sourceItemId");
        assert_eq!(
            build(&candidate, &"a".repeat(64), None).unwrap_err(),
            "diagnostic_glb_missing_shape_source_item_id"
        );
    }

    #[test]
    fn diagnostic_glb_preserves_and_marks_degenerate_source_triangles() {
        let mut candidate = candidate();
        candidate["occurrences"][1]["shapes"][0]["triangles"]
            .as_array_mut()
            .unwrap()
            .push(json!([0, 0, 1]));
        candidate["occurrences"][1]["shapes"][0]["sourceTriangleFaceIds"]
            .as_array_mut()
            .unwrap()
            .push(json!(103));
        candidate["occurrences"][1]["shapes"][0]["sourceTriangleRgba"]
            .as_array_mut()
            .unwrap()
            .push(Value::Null);
        let output = build(&candidate, &"a".repeat(64), None).unwrap();
        assert_eq!(output.triangle_count, 3);
        assert_eq!(output.degenerate_triangle_count, 1);
        let document = document_json(&output.bytes);
        let primitives = document["meshes"][0]["primitives"].as_array().unwrap();
        assert!(primitives.iter().any(|primitive| {
            primitive["extras"]["tokito"]["degenerateTriangleOrdinals"] == json!([2])
        }));
    }

    #[test]
    fn repaired_candidate_glb_carries_only_the_pinned_unqualified_repair_context() {
        let (candidate, repair) = model14_repaired_candidate();
        let output = build(&candidate, MODEL14_SOURCE_SHA256, Some(&repair)).unwrap();
        let document = document_json(&output.bytes);
        let claim = &document["asset"]["extras"]["tokito"];
        assert_eq!(claim["sourceSha256"], MODEL14_SOURCE_SHA256);
        assert_eq!(claim["geometryQualified"], false);
        assert_eq!(claim["completeSolidVerified"], false);
        assert_eq!(claim["pcbPlacementQualified"], false);
        assert_eq!(claim["admission"]["admitted"], false);
        assert_eq!(claim["experimentalPlanarRepair"]["faceId"], 51_071);
        assert_eq!(claim["experimentalPlanarRepair"]["newFaceTriangles"], 146);

        let mut unsafe_repair = repair.clone();
        unsafe_repair["positionsChanged"] = json!(true);
        assert_eq!(
            build(&candidate, MODEL14_SOURCE_SHA256, Some(&unsafe_repair)).unwrap_err(),
            "diagnostic_glb_invalid_experimental_planar_repair_context"
        );
        let mut wrong_repair_count = repair.clone();
        wrong_repair_count["newFaceTriangles"] = json!(145);
        assert_eq!(
            build(&candidate, MODEL14_SOURCE_SHA256, Some(&wrong_repair_count)).unwrap_err(),
            "diagnostic_glb_invalid_experimental_planar_repair_context"
        );
        let mut mismatched_candidate = candidate.clone();
        mismatched_candidate["occurrences"][1]["shapes"][0]["sourceTriangleFaceIds"] =
            json!(vec![101; 146]);
        assert_eq!(
            build(&mismatched_candidate, MODEL14_SOURCE_SHA256, Some(&repair)).unwrap_err(),
            "diagnostic_glb_repair_target_face_triangle_count_mismatch"
        );
        let mut mismatched_digest = candidate.clone();
        mismatched_digest["sourceSha256"] = json!("a".repeat(64));
        assert_eq!(
            build(&mismatched_digest, MODEL14_SOURCE_SHA256, Some(&repair)).unwrap_err(),
            "diagnostic_glb_repair_candidate_provenance_mismatch"
        );
        assert_eq!(
            write_repaired_candidate(
                &candidate,
                &"a".repeat(64),
                Path::new("must-not-be-written.glb"),
                &repair,
            )
            .unwrap_err(),
            "diagnostic_glb_repaired_candidate_requires_pinned_model14_source"
        );
    }
}
