#![forbid(unsafe_code)]
//! Explicit source-mesh replacements; never certifies physical geometry or pose.
mod strict_json;
use gltf::binary::{Glb, Header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    io::{Read, Write},
    path::{Component, Path},
};
use tokito_cli::digital_twin::ValidatedTwin;

type Result<T> = std::result::Result<T, String>;
const MAX_GLB: usize = 64 * 1024 * 1024;
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|e| e.to_string())?;
    if b.len() > limit {
        return Err("input_byte_limit".into());
    }
    Ok(b)
}
fn parse(b: &[u8]) -> Result<Value> {
    let text = std::str::from_utf8(b).map_err(|e| e.to_string())?;
    strict_json::parse(text).map_err(|e| e.to_string())
}
fn n(v: &Value) -> Result<f64> {
    v.as_f64()
        .filter(|x| x.is_finite() && x.abs() <= 1e6)
        .ok_or("invalid_number".into())
}
fn vector(v: &Value) -> Result<[f64; 3]> {
    let a = v
        .as_array()
        .filter(|v| v.len() == 3)
        .ok_or("invalid_vec3")?;
    Ok([n(&a[0])?, n(&a[1])?, n(&a[2])?])
}
fn label<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() < 4096 && !s.chars().any(char::is_control))
        .ok_or(format!("missing_label:{key}"))
}
fn keys(v: &Value, allowed: &[&str]) -> Result<()> {
    if v.as_object()
        .ok_or("object_required")?
        .keys()
        .any(|k| !allowed.contains(&k.as_str()))
    {
        return Err("unsupported_spec_field".into());
    }
    Ok(())
}
fn relative(base: &Path, name: &str) -> Result<std::path::PathBuf> {
    let p = Path::new(name);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) || p.as_os_str().is_empty() {
        return Err("unsafe_relative_report_path".into());
    }
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let path = base.join(p).canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&base) {
        return Err("report_path_escape".into());
    }
    Ok(path)
}

type Matrix = [[f64; 4]; 4];
fn identity() -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| if r == c { 1. } else { 0. }))
}
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|k| a[r][k] * b[k][c]).sum()))
}
fn transform(m: Matrix, p: [f64; 3]) -> Result<[f64; 3]> {
    let out = std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3]);
    if out.iter().any(|x: &f64| !x.is_finite() || x.abs() > 10.) {
        return Err("world_position_limit".into());
    }
    Ok(out)
}
fn local(node: &Value) -> Result<Matrix> {
    if let Some(a) = node.get("matrix") {
        if node.get("translation").is_some()
            || node.get("rotation").is_some()
            || node.get("scale").is_some()
        {
            return Err("mixed_matrix_trs".into());
        }
        let a = a
            .as_array()
            .filter(|a| a.len() == 16)
            .ok_or("invalid_matrix")?;
        let mut m = identity();
        for r in 0..4 {
            for c in 0..4 {
                m[r][c] = n(&a[c * 4 + r])?;
            }
        }
        if m[3] != [0., 0., 0., 1.] {
            return Err("nonaffine_matrix".into());
        }
        return Ok(m);
    }
    let t = node
        .get("translation")
        .map(vector)
        .transpose()?
        .unwrap_or([0.; 3]);
    let s = node
        .get("scale")
        .map(vector)
        .transpose()?
        .unwrap_or([1.; 3]);
    let mut m = identity();
    if let Some(q) = node.get("rotation") {
        let a = q
            .as_array()
            .filter(|a| a.len() == 4)
            .ok_or("invalid_quaternion")?;
        let [x, y, z, w] = [n(&a[0])?, n(&a[1])?, n(&a[2])?, n(&a[3])?];
        if (x * x + y * y + z * z + w * w - 1.).abs() > 1e-6 {
            return Err("nonunit_quaternion".into());
        }
        m[0][0] = 1. - 2. * (y * y + z * z);
        m[0][1] = 2. * (x * y - z * w);
        m[0][2] = 2. * (x * z + y * w);
        m[1][0] = 2. * (x * y + z * w);
        m[1][1] = 1. - 2. * (x * x + z * z);
        m[1][2] = 2. * (y * z - x * w);
        m[2][0] = 2. * (x * z - y * w);
        m[2][1] = 2. * (y * z + x * w);
        m[2][2] = 1. - 2. * (x * x + y * y);
    }
    for r in 0..3 {
        for (c, factor) in s.iter().enumerate() {
            m[r][c] *= factor;
        }
        m[r][3] = t[r];
    }
    Ok(m)
}
fn world(nodes: &[Value], index: usize, parents: &[Option<usize>]) -> Result<Matrix> {
    let mut chain = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = Some(index);
    while let Some(i) = current {
        if !seen.insert(i) || chain.len() >= 128 {
            return Err("node_cycle_or_depth_limit".into());
        }
        chain.push(i);
        current = parents[i];
    }
    let mut m = identity();
    for i in chain.into_iter().rev() {
        m = multiply(m, local(&nodes[i])?);
    }
    Ok(m)
}

fn exclusive_placement(nodes: &[Value], placement: usize, leaf: usize) -> Result<()> {
    let mut pending = vec![placement];
    let mut visited = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if !visited.insert(index) || visited.len() > 100_000 {
            return Err("placement_subtree_cycle_or_limit".into());
        }
        let node = nodes.get(index).ok_or("invalid_placement_child")?;
        if index != leaf
            && (node.get("mesh").is_some()
                || node["name"].as_str().is_some_and(|s| s.starts_with("ref:")))
        {
            return Err("placement_override_changes_unrelated_geometry".into());
        }
        if let Some(children) = node["children"].as_array() {
            for child in children {
                pending.push(child.as_u64().ok_or("invalid_placement_child")? as usize);
            }
        }
    }
    if !visited.contains(&leaf) {
        return Err("placement_override_not_ancestor".into());
    }
    Ok(())
}

#[derive(Default)]
struct Mesh {
    p: Vec<[f32; 3]>,
    n: Vec<[f32; 3]>,
    c: Vec<[f32; 4]>,
    omitted: Vec<Value>,
    source_triangles: usize,
}
fn source_mesh(report: &Value, step_sha: &str, omit_exact_zero: bool) -> Result<Mesh> {
    let candidate = &report["candidateMeshData"];
    if report["sourceSha256"] != step_sha || candidate["sourceSha256"] != step_sha {
        return Err("step_source_hash_mismatch".into());
    }
    if candidate["schema"] != "tokito.step-positioned-candidate-mesh.v1"
        || candidate["lengthUnit"] != "mm"
        || report["assemblyOccurrenceWorld"]["transformCompositionResolved"] != true
    {
        return Err("unresolved_mesh_units_or_occurrences".into());
    }
    if !candidate["sourceOccurrenceColorOverrides"]
        .as_array()
        .is_some_and(|v| v.is_empty())
    {
        return Err("occurrence_color_overrides_unsupported".into());
    }
    let mut mesh = Mesh::default();
    let occurrences = candidate["occurrences"]
        .as_array()
        .filter(|v| v.len() <= 100_000)
        .ok_or("missing_occurrences")?;
    for (occurrence_index, occurrence) in occurrences.iter().enumerate() {
        for (shape_index, shape) in occurrence["shapes"]
            .as_array()
            .ok_or("missing_shapes")?
            .iter()
            .enumerate()
        {
            let vertices = shape["verticesMm"]
                .as_array()
                .filter(|v| v.len() <= 1_000_000)
                .ok_or("invalid_vertices")?;
            let triangles = shape["triangles"]
                .as_array()
                .filter(|v| v.len() <= 2_000_000)
                .ok_or("invalid_triangles")?;
            let colors = shape["sourceTriangleRgba"]
                .as_array()
                .filter(|v| v.len() == triangles.len())
                .ok_or("missing_aligned_source_colors")?;
            if shape["triangleColorProvenanceAligned"] != true {
                return Err("unresolved_triangle_color_semantics".into());
            }
            for (triangle_index, (triangle, color)) in triangles.iter().zip(colors).enumerate() {
                mesh.source_triangles += 1;
                if mesh.source_triangles > 2_000_000 {
                    return Err("source_triangle_budget".into());
                }
                if mesh.p.len() >= 6_000_000 {
                    return Err("aggregate_triangle_limit".into());
                }
                let ids = triangle
                    .as_array()
                    .filter(|v| v.len() == 3)
                    .ok_or("invalid_triangle")?;
                let mut p = [[0.; 3]; 3];
                for i in 0..3 {
                    let v = ids[i]
                        .as_u64()
                        .and_then(|v| vertices.get(v as usize))
                        .ok_or("invalid_vertex_index")?;
                    p[i] = vector(v)?;
                }
                let a = std::array::from_fn::<_, 3, _>(|i| p[1][i] - p[0][i]);
                let b = std::array::from_fn::<_, 3, _>(|i| p[2][i] - p[0][i]);
                let normal = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
                let rgba = color
                    .as_array()
                    .filter(|v| v.len() == 4)
                    .ok_or("missing_source_color")?;
                let mut rgba_out = [0.; 4];
                for i in 0..4 {
                    let v = n(&rgba[i])?;
                    if !(0.0..=1.0).contains(&v) {
                        return Err("invalid_source_color".into());
                    }
                    rgba_out[i] = v as f32;
                }
                if normal == [0.; 3] {
                    if !omit_exact_zero {
                        return Err("exact_zero_source_triangle_requires_explicit_omission".into());
                    }
                    mesh.omitted.push(json!({"occurrenceIndex":occurrence_index,"occurrencePath":occurrence["occurrencePath"],"shapeIndex":shape_index,"sourceItemId":shape["sourceItemId"],"triangleIndex":triangle_index,"sourceFaceId":shape["sourceTriangleFaceIds"].get(triangle_index),"sourceVertexIndices":triangle,"reason":"exact_source_mm_f64_crosszero"}));
                    continue;
                }
                if !length.is_finite() || length <= 1e-14 {
                    return Err("tiny_nonzero_source_triangle_unsupported".into());
                }
                // Classify source degeneracy before changing units. Applying
                // the metre scale first can turn exact collinearity into an
                // arithmetic sliver. Rotate the source normal, scale positions once.
                let normal = [normal[0], normal[2], -normal[1]].map(|v| (v / length) as f32);
                for [x, y, z] in p {
                    mesh.p
                        .push([(x * 0.001) as f32, (z * 0.001) as f32, (-y * 0.001) as f32]);
                    mesh.n.push(normal);
                    mesh.c.push(rgba_out);
                }
            }
        }
    }
    if mesh.p.is_empty() {
        return Err("empty_source_geometry".into());
    }
    Ok(mesh)
}

fn append<const N: usize>(
    doc: &mut Value,
    bin: &mut Vec<u8>,
    data: &[[f32; N]],
    kind: &str,
    bounds: bool,
) -> Result<usize> {
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let offset = bin.len();
    for p in data {
        for v in p {
            bin.extend(v.to_le_bytes());
        }
    }
    if bin.len() > MAX_GLB {
        return Err("output_glb_budget".into());
    }
    let views = doc["bufferViews"]
        .as_array_mut()
        .ok_or("missing_buffer_views")?;
    let view = views.len();
    views
        .push(json!({"buffer":0,"byteOffset":offset,"byteLength":bin.len()-offset,"target":34962}));
    let mut accessor =
        json!({"bufferView":view,"componentType":5126,"count":data.len(),"type":kind});
    if bounds {
        accessor["min"] = json!(
            (0..N)
                .map(|i| data.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min))
                .collect::<Vec<_>>()
        );
        accessor["max"] = json!(
            (0..N)
                .map(|i| data.iter().map(|p| p[i]).fold(f32::NEG_INFINITY, f32::max))
                .collect::<Vec<_>>()
        );
    }
    let accessors = doc["accessors"].as_array_mut().ok_or("missing_accessors")?;
    let index = accessors.len();
    accessors.push(accessor);
    Ok(index)
}

fn actual_bounds(
    doc: &Value,
    bin: &[u8],
    parents: &[Option<usize>],
) -> Result<([f64; 3], [f64; 3])> {
    let nodes = doc["nodes"].as_array().ok_or("missing_nodes")?;
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    let mut vertices = 0usize;
    for (index, node) in nodes.iter().enumerate() {
        let Some(mesh_index) = node["mesh"].as_u64() else {
            continue;
        };
        let matrix = world(nodes, index, parents)?;
        let primitives = doc["meshes"]
            .get(mesh_index as usize)
            .and_then(|m| m["primitives"].as_array())
            .ok_or("missing_mesh_primitives")?;
        for primitive in primitives {
            let accessor_index = primitive["attributes"]["POSITION"]
                .as_u64()
                .ok_or("missing_position_accessor")? as usize;
            let accessor = doc["accessors"]
                .get(accessor_index)
                .ok_or("invalid_accessor_index")?;
            if accessor["componentType"] != 5126
                || accessor["type"] != "VEC3"
                || accessor.get("sparse").is_some()
            {
                return Err("unsupported_baseline_position_layout".into());
            }
            let view_index = accessor["bufferView"]
                .as_u64()
                .ok_or("missing_position_view")? as usize;
            let view = doc["bufferViews"]
                .get(view_index)
                .ok_or("invalid_position_view")?;
            if view["buffer"] != 0 {
                return Err("external_position_buffer_unsupported".into());
            }
            let view_offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
            let view_length = view["byteLength"]
                .as_u64()
                .ok_or("missing_position_view_length")? as usize;
            let accessor_offset = accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
            let stride = view["byteStride"].as_u64().unwrap_or(12) as usize;
            let count = accessor["count"]
                .as_u64()
                .filter(|n| *n <= 2_000_000)
                .ok_or("position_count_limit")? as usize;
            vertices += count;
            if vertices > 4_000_000 || !(12..=252).contains(&stride) {
                return Err("position_work_limit".into());
            }
            for i in 0..count {
                let local_offset = accessor_offset
                    .checked_add(i.checked_mul(stride).ok_or("position_offset_overflow")?)
                    .ok_or("position_offset_overflow")?;
                if local_offset
                    .checked_add(12)
                    .is_none_or(|end| end > view_length)
                {
                    return Err("position_outside_view".into());
                }
                let offset = view_offset
                    .checked_add(local_offset)
                    .ok_or("position_offset_overflow")?;
                let bytes = bin
                    .get(offset..offset.checked_add(12).ok_or("position_offset_overflow")?)
                    .ok_or("position_outside_bin")?;
                let p = std::array::from_fn(|axis| {
                    f32::from_le_bytes(bytes[axis * 4..axis * 4 + 4].try_into().unwrap()) as f64
                });
                let p = transform(matrix, p)?;
                for axis in 0..3 {
                    low[axis] = low[axis].min(p[axis]);
                    high[axis] = high[axis].max(p[axis]);
                }
            }
        }
    }
    if low.iter().any(|v| !v.is_finite()) {
        return Err("missing_renderable_bounds".into());
    }
    Ok((low, high))
}

fn execute(
    glb_path: &Path,
    map_path: &Path,
    manifest_path: &Path,
    spec_path: &Path,
    out: &Path,
) -> Result<Value> {
    if out.exists() {
        return Err("output_exists".into());
    }
    let baseline_bytes = read(glb_path, MAX_GLB)?;
    let baseline_hash = hash(&baseline_bytes);
    let baseline = ValidatedTwin::from_bytes(
        baseline_bytes,
        &read(map_path, 4 * 1024 * 1024)?,
        &read(manifest_path, 64 * 1024)?,
    )
    .map_err(|e| e.to_string())?
    .prepare(true)
    .map_err(|e| e.to_string())?;
    let spec = parse(&read(spec_path, 1024 * 1024)?)?;
    keys(
        &spec,
        &[
            "schema",
            "baselineGlbSha256",
            "models",
            "materialOverrides",
            "populationOverrides",
        ],
    )?;
    if spec["schema"] != "tokito.twin-refinement.v1" || spec["baselineGlbSha256"] != baseline_hash {
        return Err("spec_schema_or_baseline_hash_mismatch".into());
    }
    let parsed = Glb::from_slice(&baseline.glb).map_err(|e| e.to_string())?;
    let mut doc = parse(&parsed.json)?;
    let baseline_material_count = doc["materials"].as_array().map_or(0, Vec::len);
    let mut bin = parsed.bin.as_ref().ok_or("embedded_bin_required")?.to_vec();
    let original_bin = bin.clone();
    let mut map = baseline.component_map;
    let mut manifest = baseline.manifest;
    let nodes = doc["nodes"]
        .as_array()
        .filter(|n| n.len() <= 100_000)
        .ok_or("invalid_nodes")?;
    let mut parents = vec![None; nodes.len()];
    for (i, node) in nodes.iter().enumerate() {
        if let Some(children) = node["children"].as_array() {
            for child in children {
                let j = child
                    .as_u64()
                    .filter(|j| (*j as usize) < nodes.len())
                    .ok_or("invalid_child")? as usize;
                if parents[j].replace(i).is_some() {
                    return Err("multiple_node_parents".into());
                }
            }
        }
    }
    for i in 0..nodes.len() {
        world(nodes, i, &parents)?;
    }
    let mut minimum = [
        n(&manifest["boundsMetres"]["minimum"]["x"])?,
        n(&manifest["boundsMetres"]["minimum"]["y"])?,
        n(&manifest["boundsMetres"]["minimum"]["z"])?,
    ];
    let mut maximum = [
        n(&manifest["boundsMetres"]["maximum"]["x"])?,
        n(&manifest["boundsMetres"]["maximum"]["y"])?,
        n(&manifest["boundsMetres"]["maximum"]["z"])?,
    ];
    let models = spec["models"]
        .as_array()
        .filter(|a| a.len() <= 64)
        .ok_or("invalid_models")?;
    let mut seen = BTreeSet::new();
    let mut replacements = Vec::new();
    let mut placement_changes = Vec::new();
    let mut placement_seen = BTreeSet::new();
    let mut aggregate_triangles = 0usize;
    for model in models {
        keys(
            model,
            &[
                "reference",
                "meshReport",
                "meshReportSha256",
                "stepSha256",
                "localTranslationMm",
                "sourceUrl",
                "mpn",
                "declaredAxes",
                "reason",
                "placementOverride",
                "omitExactZeroAreaTriangles",
            ],
        )?;
        let reference = label(model, "reference")?;
        if !seen.insert(reference) {
            return Err("duplicate_replacement_reference".into());
        }
        if model["declaredAxes"] != "STEP_XYZ_TO_GLTF_X_Z_NEG_Y" {
            return Err("unsupported_declared_axes".into());
        }
        for key in ["sourceUrl", "mpn", "reason"] {
            label(model, key)?;
        }
        let selector = format!("ref:{reference}");
        let candidates: Vec<_> = doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, n)| n["name"] == selector)
            .map(|(i, _)| i)
            .collect();
        if candidates.len() != 1 {
            return Err("missing_or_duplicate_selector".into());
        }
        let index = candidates[0];
        if let Some(override_spec) = model.get("placementOverride") {
            keys(
                override_spec,
                &[
                    "nodeIndex",
                    "expectedTranslationMetres",
                    "translationMetres",
                    "sourceSha256",
                    "sourceLocator",
                    "reason",
                ],
            )?;
            let placement = override_spec["nodeIndex"]
                .as_u64()
                .filter(|i| (*i as usize) < parents.len())
                .ok_or("invalid_placement_node")? as usize;
            if !placement_seen.insert(placement) {
                return Err("duplicate_placement_override".into());
            }
            let mut ancestor = parents[index];
            let mut found = false;
            while let Some(i) = ancestor {
                if i == placement {
                    found = true;
                    break;
                }
                ancestor = parents[i];
            }
            if !found {
                return Err("placement_override_not_ancestor".into());
            }
            exclusive_placement(doc["nodes"].as_array().unwrap(), placement, index)?;
            let before = doc["nodes"][placement].clone();
            if before.get("matrix").is_some()
                || before["translation"] != override_spec["expectedTranslationMetres"]
            {
                return Err("placement_override_baseline_mismatch".into());
            }
            vector(&override_spec["expectedTranslationMetres"])?;
            let translation = vector(&override_spec["translationMetres"])?;
            if translation.iter().any(|x| x.abs() > 10.) {
                return Err("placement_translation_limit".into());
            }
            let digest = label(override_spec, "sourceSha256")?;
            if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("invalid_placement_source_digest".into());
            }
            for key in ["sourceLocator", "reason"] {
                label(override_spec, key)?;
            }
            doc["nodes"][placement]["translation"] = json!(translation);
            placement_changes.push(json!({"spec":override_spec,"previousNode":before,"replacementNode":doc["nodes"][placement],"poseVerified":false}));
        }
        let before = doc["nodes"][index].clone();
        if before.get("mesh").is_none()
            || ["matrix", "rotation", "scale", "children", "skin", "weights"]
                .iter()
                .any(|k| before.get(k).is_some())
        {
            return Err("replacement_requires_untransformed_leaf_selector".into());
        }
        let path = relative(
            spec_path.parent().ok_or("spec_parent_missing")?,
            label(model, "meshReport")?,
        )?;
        let bytes = read(&path, 128 * 1024 * 1024)?;
        if model["meshReportSha256"] != hash(&bytes) {
            return Err("mesh_report_hash_mismatch".into());
        }
        let report = parse(&bytes)?;
        let omit_exact_zero = match model.get("omitExactZeroAreaTriangles") {
            None => false,
            Some(value) => value.as_bool().ok_or("invalid_exact_zero_omission_flag")?,
        };
        let mesh = source_mesh(&report, label(model, "stepSha256")?, omit_exact_zero)?;
        aggregate_triangles += mesh.p.len() / 3;
        if aggregate_triangles > 2_000_000 {
            return Err("aggregate_triangle_limit".into());
        }
        let [x, y, z] = vector(&model["localTranslationMm"])?;
        let translation = [x * 0.001, z * 0.001, -y * 0.001];
        doc["nodes"][index]["translation"] = json!(translation);
        let matrix = world(doc["nodes"].as_array().unwrap(), index, &parents)?;
        for &vertex in &mesh.p {
            let p = transform(matrix, vertex.map(f64::from))?;
            for i in 0..3 {
                minimum[i] = minimum[i].min(p[i]);
                maximum[i] = maximum[i].max(p[i]);
            }
        }
        let p = append(&mut doc, &mut bin, &mesh.p, "VEC3", true)?;
        let normal = append(&mut doc, &mut bin, &mesh.n, "VEC3", false)?;
        let colors = append(&mut doc, &mut bin, &mesh.c, "VEC4", false)?;
        if !doc["materials"].is_array() {
            doc["materials"] = json!([])
        }
        let material = doc["materials"].as_array().unwrap().len();
        doc["materials"].as_array_mut().unwrap().push(json!({"name":"source STEP colours; reflectance approximation","pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1],"metallicFactor":0.15,"roughnessFactor":0.45},"doubleSided":true}));
        let meshes = doc["meshes"].as_array_mut().ok_or("missing_meshes")?;
        let mesh_index = meshes.len();
        meshes.push(json!({"name":selector,"primitives":[{"attributes":{"POSITION":p,"NORMAL":normal,"COLOR_0":colors},"material":material,"mode":4}]}));
        doc["nodes"][index]["mesh"] = json!(mesh_index);
        let matching: Vec<_> = map["components"]
            .as_array()
            .ok_or("missing_components")?
            .iter()
            .enumerate()
            .filter(|(_, row)| row["reference"] == reference && row["glb_selector"] == selector)
            .map(|(i, _)| i)
            .collect();
        if matching.len() != 1 {
            return Err("map_selector_mismatch".into());
        }
        let row = &mut map["components"][matching[0]];
        row["previous_model_file"] = row["model_file"].clone();
        row["previous_model_quality"] = row["model_quality"].clone();
        row["model_file"] = json!(format!("sha256:{}", label(model, "stepSha256")?));
        row["model_source_url"] = model["sourceUrl"].clone();
        row["model_quality"] = json!("source_step_candidate_unqualified");
        row["refinement_source"] = model.clone();
        row["identity_verified"] = json!(false);
        if !doc["nodes"][index]["extras"].is_object() {
            doc["nodes"][index]["extras"] = json!({});
        }
        doc["nodes"][index]["extras"]["tokito_refinement"] = json!({"reference":reference,"activeStepSha256":model["stepSha256"],"sourceUrl":model["sourceUrl"],"geometryQualified":false,"parentExtraMetadataHistorical":true});
        replacements.push(json!({"reference":reference,"source":model,"sourceTriangleCount":mesh.source_triangles,"triangleCount":mesh.p.len()/3,"omittedExactZeroAreaTriangleCount":mesh.omitted.len(),"omittedExactZeroAreaTriangles":mesh.omitted,"previousNode":before,"replacementNode":doc["nodes"][index],"sourceWarnings":report["warningDetails"],"sourceWarningsTotal":report["warningsTotal"],"sourceSurfaceCompletenessVerified":false,"geometryQualified":false,"poseVerified":false}));
    }
    let mut population_changes = Vec::new();
    let mut population_seen = BTreeSet::new();
    if let Some(population) = spec.get("populationOverrides") {
        for override_spec in population
            .as_array()
            .filter(|a| a.len() <= 64)
            .ok_or("invalid_population_overrides")?
        {
            keys(
                override_spec,
                &[
                    "reference",
                    "assemblyState",
                    "sourceSha256",
                    "bomItemId",
                    "sourceLocator",
                    "reason",
                    "evidenceFile",
                    "evidenceSha256",
                ],
            )?;
            let reference = label(override_spec, "reference")?;
            if !population_seen.insert(reference) || seen.contains(reference) {
                return Err("duplicate_or_conflicting_population_override".into());
            }
            if override_spec["assemblyState"] != "not_fitted" {
                return Err("unsupported_population_state".into());
            }
            let source_digest = label(override_spec, "sourceSha256")?;
            if source_digest.len() != 64 || !source_digest.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err("invalid_population_source_digest".into());
            }
            for key in ["bomItemId", "sourceLocator", "reason"] {
                label(override_spec, key)?;
            }
            let evidence_path = relative(
                spec_path.parent().ok_or("spec_parent_missing")?,
                label(override_spec, "evidenceFile")?,
            )?;
            let evidence_bytes = read(&evidence_path, 8 * 1024 * 1024)?;
            if override_spec["evidenceSha256"] != hash(&evidence_bytes) {
                return Err("population_evidence_hash_mismatch".into());
            }
            let evidence = parse(&evidence_bytes)?;
            if evidence["schema_version"] != "1.0" {
                return Err("unsupported_population_evidence_schema".into());
            }
            let matching: Vec<_> = evidence["components"]
                .as_array()
                .ok_or("population_components_missing")?
                .iter()
                .filter(|row| {
                    row["refdes"] == reference && row["bom_item_id"] == override_spec["bomItemId"]
                })
                .collect();
            if matching.len() != 1
                || matching[0]["dnp_explicit_in_bom"] != true
                || matching[0]["bom_columns"]["ASSY_OPT"] != "DNP"
            {
                return Err("population_not_explicit_dnp".into());
            }
            if source_digest != hash(&evidence_bytes)
                && !evidence["sources"]
                    .as_object()
                    .ok_or("population_sources_missing")?
                    .values()
                    .any(|source| source["sha256"] == source_digest)
            {
                return Err("population_bom_source_digest_not_bound".into());
            }
            let selector = format!("ref:{reference}");
            let node_matches: Vec<_> = doc["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, n)| n["name"] == selector)
                .map(|(i, _)| i)
                .collect();
            if node_matches.len() != 1 {
                return Err("population_selector_missing_or_duplicate".into());
            }
            let index = node_matches[0];
            let before = doc["nodes"][index].clone();
            if before.get("mesh").is_none() || before.get("children").is_some() {
                return Err("population_requires_direct_leaf_mesh".into());
            }
            doc["nodes"][index].as_object_mut().unwrap().remove("mesh");
            doc["nodes"][index]["name"] = json!(format!("population-unfitted:{reference}"));
            if !doc["nodes"][index]["extras"].is_object() {
                doc["nodes"][index]["extras"] = json!({});
            }
            doc["nodes"][index]["extras"]["tokito_population"] = json!({"reference":reference,"assemblyState":"not_fitted","source":override_spec,"physicalPopulationVerified":false});
            let rows: Vec<_> = map["components"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .filter(|(_, r)| r["reference"] == reference && r["glb_selector"] == selector)
                .map(|(i, _)| i)
                .collect();
            if rows.len() != 1 {
                return Err("population_map_selector_mismatch".into());
            }
            let row = &mut map["components"][rows[0]];
            row["previous_model_file"] = row["model_file"].clone();
            row["previous_model_quality"] = row["model_quality"].clone();
            row["glb_selector"] = Value::Null;
            row["needs_3d_body"] = json!(false);
            row["category"] = json!("unpopulated_footprint");
            row["assembly_state"] = json!("not_fitted");
            row["model_file"] = Value::Null;
            row["model_quality"] = json!("not_fitted_by_explicit_source_override");
            row["nonfittedFootprintSource"] = override_spec.clone();
            population_changes.push(json!({"reference":reference,"source":override_spec,"evidenceRow":matching[0],"previousNode":before,"replacementNode":doc["nodes"][index],"physicalPopulationVerified":false}));
        }
    }
    map["renderable_count"] = json!(
        map["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| !row["glb_selector"].is_null())
            .count()
    );
    let mut overrides = Vec::new();
    let mut overridden_materials = BTreeSet::new();
    if let Some(materials) = spec.get("materialOverrides") {
        for material in materials
            .as_array()
            .filter(|m| m.len() <= 64)
            .ok_or("invalid_material_overrides")?
        {
            keys(
                material,
                &[
                    "index",
                    "expectedName",
                    "baseColor",
                    "metallicFactor",
                    "roughnessFactor",
                    "reason",
                    "sourceUrl",
                ],
            )?;
            let index = material["index"].as_u64().ok_or("invalid_material_index")? as usize;
            if index >= baseline_material_count || !overridden_materials.insert(index) {
                return Err("material_override_not_unique_baseline_index".into());
            }
            let value = doc["materials"]
                .as_array_mut()
                .and_then(|m| m.get_mut(index))
                .ok_or("material_index_out_of_range")?;
            if value["name"] != label(material, "expectedName")? {
                return Err("material_name_mismatch".into());
            }
            for key in ["reason", "sourceUrl"] {
                label(material, key)?;
            }
            let rgba = material["baseColor"]
                .as_array()
                .filter(|a| a.len() == 4)
                .ok_or("invalid_base_color")?;
            for v in rgba {
                if !(0.0..=1.0).contains(&n(v)?) {
                    return Err("invalid_material_color".into());
                }
            }
            for key in ["metallicFactor", "roughnessFactor"] {
                if !(0.0..=1.0).contains(&n(&material[key])?) {
                    return Err("invalid_material_factor".into());
                }
            }
            value["pbrMetallicRoughness"]["baseColorFactor"] = material["baseColor"].clone();
            value["pbrMetallicRoughness"]["metallicFactor"] = material["metallicFactor"].clone();
            value["pbrMetallicRoughness"]["roughnessFactor"] = material["roughnessFactor"].clone();
            overrides.push(material.clone());
        }
    }
    (minimum, maximum) = actual_bounds(&doc, &bin, &parents)?;
    let geometry_bounds = json!({"minimum":minimum,"maximum":maximum});
    // Preserve the separately declared board registration envelope. Baseline
    // admission checked it; float mesh edges can sit infinitesimally inside
    // these engineering coordinates. This union adds contract bounds, not mesh.
    if let Some(plane) = manifest
        .get("boardPlaneBoundsMetres")
        .filter(|v| !v.is_null())
    {
        for (i, key) in ["x", "y", "z"].iter().enumerate() {
            minimum[i] = minimum[i].min(n(&plane["minimum"][key])?);
            maximum[i] = maximum[i].max(n(&plane["maximum"][key])?);
        }
    }
    if let Some(corners) = manifest
        .get("boardPlaneCornerPointsMetres")
        .and_then(Value::as_array)
    {
        for corner in corners {
            for (i, key) in ["x", "y", "z"].iter().enumerate() {
                let coordinate = n(&corner[key])?;
                minimum[i] = minimum[i].min(coordinate);
                maximum[i] = maximum[i].max(coordinate);
            }
        }
    }
    doc["buffers"][0]["byteLength"] = json!(bin.len());
    if !bin.starts_with(&original_bin) {
        return Err("binary_prefix_changed".into());
    }
    let bytes = Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(&doc).map_err(|e| e.to_string())?),
        bin: Some(Cow::Owned(bin)),
    }
    .to_vec()
    .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_GLB {
        return Err("output_glb_budget".into());
    }
    let digest = hash(&bytes);
    let asset = format!("tokito-refined-{}", &digest[..32]);
    manifest["assetId"] = json!(asset);
    manifest["glbSha256"] = json!(format!("sha256:{digest}"));
    manifest["glbBytes"] = json!(bytes.len());
    for (name, values) in [("minimum", minimum), ("maximum", maximum)] {
        manifest["boundsMetres"][name] = json!({"x":values[0],"y":values[1],"z":values[2]});
    }
    for flag in [
        "physicalGeometryVerified",
        "arRegistrationVerified",
        "releaseEligible",
    ] {
        manifest[flag] = json!(false);
    }
    map["asset_id"] = json!(asset);
    for flag in [
        "physical_fit_verified",
        "calibration_eligible",
        "positional_guidance_allowed",
    ] {
        map[flag] = json!(false);
    }
    let map_bytes = serde_json::to_vec_pretty(&map).map_err(|e| e.to_string())?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let accepted = ValidatedTwin::from_bytes(bytes.clone(), &map_bytes, &manifest_bytes)
        .map_err(|e| e.to_string())?;
    let receipt = json!({"schema":"tokito.twin-refinement-report.v1","baselineGlbSha256":baseline_hash,"preparedBaselineGlbSha256":hash(&baseline.glb),"baselineMapSha256":hash(&read(map_path,4*1024*1024)?),"baselineManifestSha256":hash(&read(manifest_path,64*1024)?),"outputGlbSha256":digest,"replacementCount":replacements.len(),"replacements":replacements,"populationOverrideCount":population_changes.len(),"populationOverrides":population_changes,"materialOverrideCount":overrides.len(),"materialOverrides":overrides,"materialPhysicalPropertiesVerified":false,"allOtherNodeJsonPreserved":true,"outsideTargetPlacementWorldPreserved":true,"explicitPlacementOverrides":placement_changes,"originalBinaryPrefixPreserved":true,"boundsMethod":"actual emitted FLOAT VEC3 world geometry union retained declared board-plane bounds and corners","actualGeometryBoundsMetres":geometry_bounds,"retainedBoardPlaneBoundsMetres":manifest["boardPlaneBoundsMetres"],"retainedBoardPlaneCornersMetres":manifest["boardPlaneCornerPointsMetres"],"geometryQualified":false,"poseVerified":false,"physicalFitVerified":false,"positionalGuidanceAllowed":false,"selectionIndex":accepted.index_json()});
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    for (name, bytes) in [
        ("active.digital-twin.glb", bytes),
        ("active.digital-twin.json", manifest_bytes),
        ("component-map.json", map_bytes),
        (
            "prepare-report.json",
            serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
        ),
    ] {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(name))
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|e| e.to_string())?;
    }
    Ok(receipt)
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = if args.len() == 6 && args[5] == "--allow-partial" {
        execute(
            Path::new(&args[0]),
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            Path::new(&args[4]),
        )
    } else {
        Err(
            "usage: tokito-twin-refinement <GLB> <MAP> <MANIFEST> <SPEC> <NEW_OUT> --allow-partial"
                .into(),
        )
    };
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => {
            println!("{}", json!({"error":error,"admitted":false}));
            std::process::exit(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_fields_refuse() {
        assert!(keys(&json!({"unknown":1}), &["schema"]).is_err());
    }
    #[test]
    fn source_hash_missing_or_changed_refuses() {
        assert!(source_mesh(&json!({}), "hash", false).is_err());
    }
    #[test]
    fn ancestor_translation_composes() {
        let nodes = vec![
            json!({"translation":[1,2,3],"children":[1]}),
            json!({"translation":[0.1,0.2,0.3]}),
        ];
        assert_eq!(
            transform(world(&nodes, 1, &[None, Some(0)]).unwrap(), [0.; 3]).unwrap(),
            [1.1, 2.2, 3.3]
        );
    }
    #[test]
    fn cycles_refuse() {
        assert!(world(&[json!({})], 0, &[Some(0)]).is_err());
    }
    #[test]
    fn nonfinite_vectors_refuse() {
        assert!(vector(&json!([1, 2, "nan"])).is_err());
    }
    #[test]
    fn shared_placement_refuses_unrelated_world_motion() {
        let nodes = vec![
            json!({"children":[1,2]}),
            json!({"name":"ref:U8","mesh":0}),
            json!({"name":"ref:U1","mesh":1}),
        ];
        assert!(exclusive_placement(&nodes, 0, 1).is_err());
        assert!(
            exclusive_placement(
                &[json!({"children":[1]}), json!({"name":"ref:U8","mesh":0})],
                0,
                1
            )
            .is_ok()
        );
    }
    #[test]
    fn exact_zero_omission_is_explicit_and_records_source_ordinals() {
        let report = json!({"sourceSha256":"source","assemblyOccurrenceWorld":{"transformCompositionResolved":true},"candidateMeshData":{"sourceSha256":"source","schema":"tokito.step-positioned-candidate-mesh.v1","lengthUnit":"mm","sourceOccurrenceColorOverrides":[],"occurrences":[{"occurrencePath":[0],"shapes":[{"sourceItemId":42,"verticesMm":[[0,0,0],[1,0,0],[0,1,0]],"triangles":[[0,1,2],[0,0,0]],"sourceTriangleRgba":[[1,0,0,1],[1,0,0,1]],"sourceTriangleFaceIds":[9,10],"triangleColorProvenanceAligned":true}]}]}});
        assert!(source_mesh(&report, "source", false).is_err());
        let mesh = source_mesh(&report, "source", true).unwrap();
        assert_eq!(mesh.source_triangles, 2);
        assert_eq!(mesh.p.len(), 3);
        assert_eq!(mesh.omitted[0]["triangleIndex"], 1);
        assert_eq!(mesh.omitted[0]["sourceFaceId"], 10);
        let mut bad = report;
        bad["candidateMeshData"]["occurrences"][0]["shapes"][0]["sourceTriangleRgba"][1] =
            Value::Null;
        assert!(source_mesh(&bad, "source", true).is_err());
    }
    #[test]
    fn source_collinearity_is_classified_before_metre_rounding() {
        let p = [
            [39.9375, 35.4375, -55.0],
            [39.75, 35.8125, -54.5625],
            [39.5625, 36.1875, -54.125],
        ];
        let scaled = p.map(|row| row.map(|x| x * 0.001));
        let a = std::array::from_fn::<_, 3, _>(|i| scaled[1][i] - scaled[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| scaled[2][i] - scaled[0][i]);
        assert_ne!(
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0]
            ],
            [0.; 3]
        );
        let report = json!({"sourceSha256":"s","assemblyOccurrenceWorld":{"transformCompositionResolved":true},"candidateMeshData":{"sourceSha256":"s","schema":"tokito.step-positioned-candidate-mesh.v1","lengthUnit":"mm","sourceOccurrenceColorOverrides":[],"occurrences":[{"shapes":[{"verticesMm":[p[0],p[1],p[2],[0,0,0],[1,0,0],[0,1,0]],"triangles":[[0,1,2],[3,4,5]],"sourceTriangleRgba":[[1,1,1,1],[1,1,1,1]],"triangleColorProvenanceAligned":true}]}]}});
        let mesh = source_mesh(&report, "s", true).unwrap();
        assert_eq!(mesh.omitted.len(), 1);
        assert_eq!(mesh.p.len(), 3);
        assert_eq!(mesh.omitted[0]["reason"], "exact_source_mm_f64_crosszero");
    }
}
