//! Hash-bound adaptation of the dataset's source-geometry candidate profile.
//! Source frame declarations are retained claims, never physical/AR certification.
use crate::{EngineError, digital_twin::ValidatedTwin, sha256_hex, strict_json};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
};

pub struct Adapted {
    pub glb: Vec<u8>,
    pub map: Value,
    pub manifest: Value,
    pub report: Value,
}
fn err(s: impl Into<String>) -> EngineError {
    EngineError::new(s)
}
fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>, EngineError> {
    let context = |e: std::io::Error| {
        err(format!(
            "cannot read source candidate input '{}': {e}. Supply all five input files from the same pinned candidate; no output was created during input loading.",
            path.display()
        ))
    };
    let meta = std::fs::symlink_metadata(path).map_err(context)?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return Err(err(format!(
            "source candidate input is not a bounded regular file: {}",
            path.display()
        )));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(context)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(context)?;
    if bytes.len() > limit {
        return Err(err(format!(
            "source candidate input grew beyond limit: {}",
            path.display()
        )));
    }
    Ok(bytes)
}
fn parse(b: &[u8]) -> Result<Value, EngineError> {
    strict_json::parse(std::str::from_utf8(b).map_err(|e| err(e.to_string()))?)
        .map_err(|e| err(e.to_string()))
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>, EngineError> {
    v[key]
        .as_array()
        .ok_or_else(|| err(format!("missing array {key}")))
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, EngineError> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .ok_or_else(|| err(format!("missing text {key}")))
}
fn integers(v: &Value) -> Result<BTreeSet<usize>, EngineError> {
    let mut out = BTreeSet::new();
    for n in v.as_array().ok_or_else(|| err("missing ownership array"))? {
        let n = n
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| err("invalid ownership index"))?;
        if !out.insert(n) {
            return Err(err("duplicate ownership index"));
        }
    }
    Ok(out)
}
fn bind(provenance: &Value, path: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| err("invalid file name"))?;
    let claims: Vec<_> = array(provenance, "files")?
        .iter()
        .filter(|v| v["path"] == name)
        .collect();
    if claims.len() != 1
        || claims[0]["sha256"] != sha256_hex(bytes)
        || claims[0]["bytes"].as_u64() != Some(bytes.len() as u64)
    {
        return Err(err(format!("provenance hash/size mismatch for {name}")));
    }
    Ok(())
}

pub fn load(
    glb_path: &Path,
    map_path: &Path,
    index_path: &Path,
    provenance_path: &Path,
    coverage_path: &Path,
    board_id: &str,
    revision: &str,
) -> Result<Adapted, EngineError> {
    load_with_board_plane(
        glb_path,
        map_path,
        index_path,
        provenance_path,
        coverage_path,
        (board_id, revision),
        None,
    )
}

/// Adapt a source asset with an optional explicitly selected substrate subtree.
pub fn load_with_board_plane(
    glb_path: &Path,
    map_path: &Path,
    index_path: &Path,
    provenance_path: &Path,
    coverage_path: &Path,
    identity: (&str, &str),
    board_plane_node: Option<&str>,
) -> Result<Adapted, EngineError> {
    let (board_id, revision) = identity;
    let glb = bounded(glb_path, 64 * 1024 * 1024)?;
    let map_bytes = bounded(map_path, 4 * 1024 * 1024)?;
    let index_bytes = bounded(index_path, 4 * 1024 * 1024)?;
    let provenance_bytes = bounded(provenance_path, 4 * 1024 * 1024)?;
    let coverage_bytes = bounded(coverage_path, 4 * 1024 * 1024)?;
    let provenance = parse(&provenance_bytes)?;
    if provenance["schema_version"] != "1.0"
        || provenance["release_eligible"] != false
        || provenance["photo_registration_verified"] != false
        || provenance["physical_flip_verified"] != false
    {
        return Err(err(
            "unsupported provenance profile or elevated qualification",
        ));
    }
    bind(&provenance, glb_path, &glb)?;
    bind(&provenance, map_path, &map_bytes)?;
    bind(&provenance, index_path, &index_bytes)?;
    let mut map = parse(&map_bytes)?;
    let index = parse(&index_bytes)?;
    let coverage = parse(&coverage_bytes)?;
    if coverage["schema_version"] != "1.0"
        || coverage["production_eligible"] != false
        || index["release_eligible"] != false
        || index["source_geometry_only"] != true
        || index["photo_correspondence_verified"] != false
    {
        return Err(err("unsupported source profile or elevated qualification"));
    }
    if glb.len() < 20
        || &glb[..4] != b"glTF"
        || u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize != glb.len()
    {
        return Err(err("invalid GLB container framing"));
    }
    if index["output_sha256"] != sha256_hex(&glb)
        || coverage["source_index_sha256"] != sha256_hex(&index_bytes)
    {
        return Err(err("source index/coverage hash mismatch"));
    }
    let container = gltf::binary::Glb::from_slice(&glb).map_err(|e| err(e.to_string()))?;
    let document = parse(&container.json)?;
    let indexed = index["components"]
        .as_object()
        .ok_or_else(|| err("missing indexed components"))?;
    let nodes = array(&document, "nodes")?;
    let mut refs = BTreeSet::new();
    let mut owned_nodes = BTreeSet::new();
    let mut owned_meshes = BTreeSet::new();
    let components = map["components"]
        .as_array_mut()
        .ok_or_else(|| err("missing components"))?;
    if components.len() > 10_000 {
        return Err(err("component limit exceeded"));
    }
    for component in components.iter_mut() {
        let reference = text(component, "reference")?.to_owned();
        if !refs.insert(reference.clone())
            || component["component_id"] != format!("{board_id}:{revision}:{reference}")
        {
            return Err(err("duplicate reference or mismatched component identity"));
        }
        let claim = indexed
            .get(&reference)
            .ok_or_else(|| err("component absent from source index"))?;
        let selector = text(component, "glb_selector")?;
        if claim["glb_selector"] != selector || selector != format!("ref:{reference}") {
            return Err(err("selector mismatch"));
        }
        let ns = integers(&claim["node_indices"])?;
        let ms = integers(&claim["mesh_indices"])?;
        let starts: Vec<_> = nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n["name"] == selector)
            .map(|(i, _)| i)
            .collect();
        if starts.len() != 1 {
            return Err(err("selector is missing or ambiguous"));
        }
        let mut actual_nodes = BTreeSet::new();
        let mut actual_meshes = BTreeSet::new();
        let mut pending = starts;
        while let Some(i) = pending.pop() {
            if !actual_nodes.insert(i) {
                return Err(err("component hierarchy cycle/shared node"));
            }
            let n = nodes.get(i).ok_or_else(|| err("invalid component node"))?;
            if let Some(m) = n["mesh"].as_u64() {
                actual_meshes.insert(m as usize);
            }
            if let Some(children) = n.get("children") {
                pending.extend(integers(children)?);
            }
        }
        if actual_nodes != ns
            || actual_meshes != ms
            || ms.is_empty()
            || ns.iter().any(|n| !owned_nodes.insert(*n))
            || ms.iter().any(|m| !owned_meshes.insert(*m))
        {
            return Err(err("indexed component ownership differs from actual GLB"));
        }
        component["component_class"] = Value::Null;
        component["model_quality"] = json!("unverified-source-geometry");
        component["physical_geometry_verified"] = json!(false);
    }
    if indexed.len() != refs.len()
        || index["selectable_source_references"].as_u64() != Some(refs.len() as u64)
        || coverage["independently_selectable_source_bodies"].as_u64() != Some(refs.len() as u64)
    {
        return Err(err("renderable count differs"));
    }
    let expected_missing: BTreeSet<String> =
        array(&index, "references_without_selectable_geometry")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| err("invalid missing reference"))
            })
            .collect::<Result<_, _>>()?;
    if expected_missing.len() != array(&index, "references_without_selectable_geometry")?.len() {
        return Err(err("duplicate missing reference"));
    }
    let mut missing = BTreeSet::new();
    let mut categories = BTreeMap::<String, usize>::new();
    let mut missing_packages = Vec::new();
    let mut unknown_spacers = Vec::new();
    for item in array(&coverage, "references")? {
        let reference = text(item, "reference")?;
        let category = text(item, "category")?;
        if !missing.insert(reference.to_owned()) || refs.contains(reference) {
            return Err(err("duplicate/overlapping missing reference"));
        }
        *categories.entry(category.to_owned()).or_default() += 1;
        let needs_body = match category {
            "test_contact; needs_source_pad_region_not_package_body"
            | "optical_landmark; needs_source_copper_region_not_package_body"
            | "solder_jumper; needs_pad_and_bridge_population_review" => false,
            "component_package_body_missing" => {
                missing_packages.push(reference.to_owned());
                true
            }
            "mechanical_spacer; fitted_population_and_body_pending" => {
                unknown_spacers.push(reference.to_owned());
                true
            }
            _ => return Err(err("unsupported source missing-geometry category")),
        };
        components.push(json!({"component_id":format!("{board_id}:{revision}:{reference}"),"reference":reference,"glb_selector":null,"component_class":null,"needs_3d_body":needs_body,"model_quality":"missing","source_category":category,"physical_geometry_verified":false}));
    }
    if missing != expected_missing
        || coverage["unmodeled_reference_classification"] != json!(categories)
    {
        return Err(err("coverage categories/reference set differs"));
    }
    let renderable = refs.len();
    let count = components.len();
    let (bounds, plane_bounds) = world_bounds(&glb, board_plane_node)?;
    let context = sha256_hex(&provenance_bytes);
    let asset_id = format!("source-candidate-{context}");
    map["board_id"] = json!(board_id);
    map["board_revision"] = json!(revision);
    map["asset_id"] = json!(asset_id);
    map["component_count"] = json!(count);
    map["renderable_count"] = json!(renderable);
    let mut manifest = json!({"schemaVersion":"1.0","assetId":asset_id,"boardIdentity":board_id,"boardRevision":revision,"assemblyContextRevision":context,"units":"METRE","axes":"RIGHT_HANDED_Y_UP","componentNodePrefix":"ref:","glbSha256":sha256_hex(&glb),"glbBytes":glb.len(),"boardOriginMetres":{"x":0.,"y":0.,"z":0.},"boundsMetres":{"minimum":{"x":bounds[0][0],"y":bounds[0][1],"z":bounds[0][2]},"maximum":{"x":bounds[1][0],"y":bounds[1][1],"z":bounds[1][2]}},"defaultOpacity":0.5});
    if let Some(plane) = plane_bounds {
        manifest["boardPlaneBoundsMetres"] = json!({"minimum":{"x":plane[0][0],"y":plane[0][1],"z":plane[0][2]},"maximum":{"x":plane[1][0],"y":plane[1][1],"z":plane[1][2]}});
        manifest["boardPlaneSourceNode"] = json!(board_plane_node);
    }
    ValidatedTwin::from_bytes(
        glb.clone(),
        &serde_json::to_vec(&map).map_err(|e| err(e.to_string()))?,
        &serde_json::to_vec(&manifest).map_err(|e| err(e.to_string()))?,
    )?;
    let report = json!({"schema":"tokito.source-candidate-adaptation.v1","componentCount":count,"renderableCount":renderable,"missingPackageBodies":missing_packages,"unknownMechanicalSpacers":unknown_spacers,"sourceCategories":categories,"sourcePcbSha256":provenance["source_pcb_sha256"],"sourceProvenanceSha256":context,"coverageSha256":sha256_hex(&coverage_bytes),"coveragePublisherProvenanceVerified":false,"coverageOriginAuthenticated":false,"classificationSourceClaimsOnly":true,"frameDeclaration":"dataset source profile: METRE / RIGHT_HANDED_Y_UP","originMeaning":"asset-frame zero; not measured board plane","boardPlaneSourceNode":board_plane_node,"boardPlaneBoundsDecoded":plane_bounds.is_some(),"physicalGeometryVerified":false,"photoRegistrationVerified":false,"releaseEligible":false});
    Ok(Adapted {
        glb,
        map,
        manifest,
        report,
    })
}

type Matrix = [[f64; 4]; 4];
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    let mut c = [[0.; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            c[col][row] = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    c
}
fn numbers<const N: usize>(value: &Value) -> Result<[f64; N], EngineError> {
    let values = value
        .as_array()
        .filter(|v| v.len() == N)
        .ok_or_else(|| err("invalid transform array shape"))?;
    let mut result = [0.; N];
    for (i, v) in values.iter().enumerate() {
        result[i] = v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| err("nonfinite transform number"))?;
    }
    Ok(result)
}
/// Keep JSON transform precision. POSITION payloads are f32; JSON matrix/TRS
/// numbers are read as f64 without passing through gltf's f32 transform API.
/// Unit quaternions use the source values directly (no silent normalization).
fn local_matrix(node: &Value) -> Result<Matrix, EngineError> {
    let matrix = if let Some(value) = node.get("matrix") {
        if ["translation", "rotation", "scale"]
            .iter()
            .any(|k| node.get(k).is_some())
        {
            return Err(err("matrix and TRS cannot coexist"));
        }
        let flat = numbers::<16>(value)?;
        let mut matrix = [[0.; 4]; 4];
        for col in 0..4 {
            for row in 0..4 {
                matrix[col][row] = flat[col * 4 + row];
            }
        }
        matrix
    } else {
        let t = node
            .get("translation")
            .map(numbers::<3>)
            .transpose()?
            .unwrap_or([0.; 3]);
        let s = node
            .get("scale")
            .map(numbers::<3>)
            .transpose()?
            .unwrap_or([1.; 3]);
        let [x, y, z, w] = node
            .get("rotation")
            .map(numbers::<4>)
            .transpose()?
            .unwrap_or([0., 0., 0., 1.]);
        if (x * x + y * y + z * z + w * w - 1.).abs() > 1e-6 {
            return Err(err("source rotation quaternion is not unit length"));
        }
        [
            [
                (1. - 2. * (y * y + z * z)) * s[0],
                2. * (x * y + z * w) * s[0],
                2. * (x * z - y * w) * s[0],
                0.,
            ],
            [
                2. * (x * y - z * w) * s[1],
                (1. - 2. * (x * x + z * z)) * s[1],
                2. * (y * z + x * w) * s[1],
                0.,
            ],
            [
                2. * (x * z + y * w) * s[2],
                2. * (y * z - x * w) * s[2],
                (1. - 2. * (x * x + y * y)) * s[2],
                0.,
            ],
            [t[0], t[1], t[2], 1.],
        ]
    };
    if matrix.iter().flatten().any(|v| !v.is_finite())
        || matrix[0][3] != 0.
        || matrix[1][3] != 0.
        || matrix[2][3] != 0.
        || matrix[3][3] != 1.
    {
        return Err(err("requires a finite affine source transform"));
    }
    let det = matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[2][1] * matrix[1][2])
        - matrix[1][0] * (matrix[0][1] * matrix[2][2] - matrix[2][1] * matrix[0][2])
        + matrix[2][0] * (matrix[0][1] * matrix[1][2] - matrix[1][1] * matrix[0][2]);
    if !det.is_finite() || det == 0. {
        return Err(err("singular source transform"));
    }
    Ok(matrix)
}
type Bounds = [[f64; 3]; 2];
fn world_bounds(
    bytes: &[u8],
    plane_node: Option<&str>,
) -> Result<(Bounds, Option<Bounds>), EngineError> {
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| err(e.to_string()))?;
    let container = gltf::binary::Glb::from_slice(bytes).map_err(|e| err(e.to_string()))?;
    let document = parse(&container.json)?;
    let source_nodes = array(&document, "nodes")?;
    let plane_index = if let Some(name) = plane_node {
        if name.is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
            return Err(err("invalid board-plane source node name"));
        }
        let matches: Vec<_> = source_nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node["name"].as_str() == Some(name))
            .map(|(index, _)| index)
            .collect();
        if matches.len() != 1 {
            return Err(err("board-plane source node must resolve uniquely"));
        }
        Some(matches[0])
    } else {
        None
    };
    let bin = gltf
        .blob
        .as_ref()
        .ok_or_else(|| err("missing embedded buffer"))?;
    let scene = gltf
        .default_scene()
        .ok_or_else(|| err("missing default scene"))?;
    let identity = [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    let mut pending: Vec<_> = scene.nodes().map(|n| (n, identity, false)).collect();
    let mut seen = BTreeSet::new();
    let mut bounds = [[f64::INFINITY; 3], [f64::NEG_INFINITY; 3]];
    let mut plane_bounds = bounds;
    let mut plane_count = 0usize;
    let mut count = 0usize;
    while let Some((node, parent, inside_plane)) = pending.pop() {
        let inside_plane = inside_plane || plane_index == Some(node.index());
        if !seen.insert(node.index()) || seen.len() > 100_000 {
            return Err(err("shared/cyclic scene or node limit exceeded"));
        }
        let local = local_matrix(
            source_nodes
                .get(node.index())
                .ok_or_else(|| err("missing source node"))?,
        )?;
        let world = multiply(parent, local);
        if world.iter().flatten().any(|v| !v.is_finite()) {
            return Err(err("nonfinite transform"));
        }
        pending.extend(node.children().map(|n| (n, world, inside_plane)));
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                let accessor = primitive
                    .get(&gltf::Semantic::Positions)
                    .ok_or_else(|| err("primitive lacks POSITION"))?;
                if accessor.data_type() != gltf::accessor::DataType::F32
                    || accessor.dimensions() != gltf::accessor::Dimensions::Vec3
                    || accessor.sparse().is_some()
                {
                    return Err(err("requires dense float VEC3 POSITION"));
                }
                let view = accessor
                    .view()
                    .ok_or_else(|| err("missing position buffer view"))?;
                if !matches!(view.buffer().source(), gltf::buffer::Source::Bin) {
                    return Err(err("external buffers unsupported"));
                }
                let stride = view.stride().unwrap_or(12);
                if stride < 12 {
                    return Err(err("invalid position stride"));
                }
                let start = view
                    .offset()
                    .checked_add(accessor.offset())
                    .ok_or_else(|| err("position offset overflow"))?;
                for i in 0..accessor.count() {
                    if inside_plane {
                        plane_count += 1;
                    }
                    count = count
                        .checked_add(1)
                        .ok_or_else(|| err("vertex count overflow"))?;
                    if count > 10_000_000 {
                        return Err(err("decoded vertex budget exceeded"));
                    }
                    let offset = i
                        .checked_mul(stride)
                        .and_then(|n| start.checked_add(n))
                        .ok_or_else(|| err("position offset overflow"))?;
                    let end = offset
                        .checked_add(12)
                        .ok_or_else(|| err("position range overflow"))?;
                    let view_end = view
                        .offset()
                        .checked_add(view.length())
                        .ok_or_else(|| err("view range overflow"))?;
                    if end > view_end {
                        return Err(err("position outside view"));
                    }
                    let data = bin
                        .get(offset..end)
                        .ok_or_else(|| err("position outside buffer"))?;
                    let mut p = [0.; 4];
                    p[3] = 1.;
                    for k in 0..3 {
                        p[k] =
                            f32::from_le_bytes(data[k * 4..k * 4 + 4].try_into().unwrap()) as f64;
                    }
                    for axis in 0..3 {
                        let v = (0..4).map(|k| world[k][axis] * p[k]).sum::<f64>();
                        if !v.is_finite() || v.abs() > 10. {
                            return Err(err("nonfinite or oversized world geometry"));
                        }
                        bounds[0][axis] = bounds[0][axis].min(v);
                        bounds[1][axis] = bounds[1][axis].max(v);
                        if inside_plane {
                            plane_bounds[0][axis] = plane_bounds[0][axis].min(v);
                            plane_bounds[1][axis] = plane_bounds[1][axis].max(v);
                        }
                    }
                }
            }
        }
    }
    if count == 0 || (0..3).any(|i| bounds[0][i] >= bounds[1][i]) {
        return Err(err("empty or degenerate scene bounds"));
    }
    if plane_index.is_some()
        && (plane_count == 0 || (0..3).any(|i| plane_bounds[0][i] >= plane_bounds[1][i]))
    {
        return Err(err(
            "board-plane source subtree is inactive, empty or degenerate",
        ));
    }
    Ok((bounds, plane_index.map(|_| plane_bounds)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    fn fixture() -> (std::path::PathBuf, Vec<u8>) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "tokito-source-candidate-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let bin: Vec<u8> = [0f32, 0., 0., 0.01, 0., 0., 0., 0.02, 0.03]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let doc = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":bin.len()}],"bufferViews":[{"buffer":0,"byteLength":bin.len()}],"accessors":[{"bufferView":0,"componentType":5126,"type":"VEC3","count":3,"min":[0.,0.,0.],"max":[0.01,0.02,0.03]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"name":"ref:Q7","mesh":0,"translation":[0.1,0.2,0.3]}],"scenes":[{"nodes":[0]}],"scene":0});
        let glb = gltf::binary::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&doc).unwrap()),
            bin: Some(Cow::Owned(bin)),
        }
        .to_vec()
        .unwrap();
        let map=serde_json::to_vec(&json!({"components":[{"component_id":"fixture:r1:Q7","reference":"Q7","glb_selector":"ref:Q7","needs_3d_body":true}]})).unwrap();
        let index=serde_json::to_vec(&json!({"release_eligible":false,"source_geometry_only":true,"photo_correspondence_verified":false,"output_sha256":sha256_hex(&glb),"selectable_source_references":1,"references_without_selectable_geometry":["X1"],"components":{"Q7":{"glb_selector":"ref:Q7","node_indices":[0],"mesh_indices":[0]}}})).unwrap();
        let coverage = json!({"schema_version":"1.0","production_eligible":false,"source_index_sha256":sha256_hex(&index),"independently_selectable_source_bodies":1,"unmodeled_reference_classification":{"test_contact; needs_source_pad_region_not_package_body":1},"references":[{"reference":"X1","category":"test_contact; needs_source_pad_region_not_package_body"}]});
        let provenance = json!({"schema_version":"1.0","release_eligible":false,"photo_registration_verified":false,"physical_flip_verified":false,"files":[{"path":"model.glb","sha256":sha256_hex(&glb),"bytes":glb.len()},{"path":"map.json","sha256":sha256_hex(&map),"bytes":map.len()},{"path":"index.json","sha256":sha256_hex(&index),"bytes":index.len()}]});
        for (name, bytes) in [
            ("model.glb", glb.clone()),
            ("map.json", map),
            ("index.json", index),
            ("coverage.json", serde_json::to_vec(&coverage).unwrap()),
            ("provenance.json", serde_json::to_vec(&provenance).unwrap()),
        ] {
            std::fs::write(path.join(name), bytes).unwrap();
        }
        (path, glb)
    }
    fn adapt(path: &Path) -> Result<Adapted, EngineError> {
        load(
            &path.join("model.glb"),
            &path.join("map.json"),
            &path.join("index.json"),
            &path.join("provenance.json"),
            &path.join("coverage.json"),
            "fixture",
            "r1",
        )
    }
    fn edited_glb(bytes: &[u8], edit: impl FnOnce(&mut Value)) -> Vec<u8> {
        let source = gltf::binary::Glb::from_slice(bytes).unwrap();
        let mut document: Value = serde_json::from_slice(&source.json).unwrap();
        edit(&mut document);
        gltf::binary::Glb {
            header: source.header,
            json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
            bin: source.bin,
        }
        .to_vec()
        .unwrap()
    }
    #[test]
    fn plane_subtree_uses_world_transforms_without_connector_overhang() {
        let (path, bytes) = fixture();
        let bytes = edited_glb(&bytes, |doc| {
            doc["nodes"].as_array_mut().unwrap().extend([
                json!({"name":"substrate","translation":[-0.2,0.,0.],"children":[2]}),
                json!({"mesh":0,"translation":[0.,0.001,0.]}),
            ]);
            doc["scenes"][0]["nodes"] = json!([0, 1]);
        });
        let (overall, plane) = world_bounds(&bytes, Some("substrate")).unwrap();
        let plane = plane.unwrap();
        assert!((plane[0][0] + 0.2).abs() < 1e-9);
        assert!((plane[1][0] + 0.19).abs() < 1e-8);
        assert!((plane[0][1] - 0.001).abs() < 1e-9);
        assert!((overall[1][0] - 0.11).abs() < 1e-8);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn plane_selector_rejects_missing_duplicate_inactive_and_empty_nodes() {
        let (path, bytes) = fixture();
        assert!(world_bounds(&bytes, Some("missing")).is_err());
        assert!(world_bounds(&bytes, Some("\n")).is_err());
        for (nodes, roots) in [
            (
                json!([{"name":"substrate","mesh":0},{"name":"substrate","mesh":0}]),
                json!([0, 1]),
            ),
            (
                json!([{"mesh":0},{"name":"substrate","mesh":0}]),
                json!([0]),
            ),
            (json!([{"mesh":0},{"name":"substrate"}]), json!([0, 1])),
        ] {
            let invalid = edited_glb(&bytes, |doc| {
                doc["nodes"] = nodes;
                doc["scenes"][0]["nodes"] = roots;
            });
            assert!(world_bounds(&invalid, Some("substrate")).is_err());
        }
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn explicit_plane_does_not_grant_physical_qualification() {
        let (path, _) = fixture();
        let result = load_with_board_plane(
            &path.join("model.glb"),
            &path.join("map.json"),
            &path.join("index.json"),
            &path.join("provenance.json"),
            &path.join("coverage.json"),
            ("fixture", "r1"),
            Some("ref:Q7"),
        )
        .unwrap();
        assert_eq!(result.manifest["boardPlaneSourceNode"], "ref:Q7");
        assert_eq!(
            result.manifest["boardPlaneBoundsMetres"],
            result.manifest["boundsMetres"]
        );
        assert_eq!(result.report["boardPlaneBoundsDecoded"], true);
        for gate in [
            "physicalGeometryVerified",
            "photoRegistrationVerified",
            "releaseEligible",
        ] {
            assert_eq!(result.report[gate], false);
        }
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn source_ids_categories_and_decoded_translated_bounds_are_preserved() {
        let (path, glb) = fixture();
        let result = adapt(&path).unwrap();
        assert_eq!(result.glb, glb);
        assert_eq!(result.map["component_count"], 2);
        assert!(result.map["components"][0]["component_class"].is_null());
        assert_eq!(result.map["components"][1]["needs_3d_body"], false);
        assert!(
            (result.manifest["boundsMetres"]["minimum"]["x"]
                .as_f64()
                .unwrap()
                - 0.1)
                .abs()
                < 1e-7
        );
        assert!(
            (result.manifest["boundsMetres"]["maximum"]["z"]
                .as_f64()
                .unwrap()
                - 0.33)
                .abs()
                < 1e-7
        );
        assert_eq!(result.report["physicalGeometryVerified"], false);
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn changed_payload_and_unknown_categories_fail_closed() {
        let (path, _) = fixture();
        let mut coverage: Value =
            serde_json::from_slice(&std::fs::read(path.join("coverage.json")).unwrap()).unwrap();
        coverage["references"][0]["category"] = json!("guessed resistor");
        std::fs::write(
            path.join("coverage.json"),
            serde_json::to_vec(&coverage).unwrap(),
        )
        .unwrap();
        assert!(
            adapt(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("unsupported")
        );
        std::fs::write(path.join("model.glb"), b"tampered").unwrap();
        assert!(
            adapt(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("hash/size")
        );
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn multiplication_respects_column_major_translation_rotation() {
        let translation = [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [1., 2., 3., 1.],
        ];
        let rotation = [
            [0., 1., 0., 0.],
            [-1., 0., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ];
        assert_eq!(multiply(translation, rotation)[3], [1., 2., 3., 1.]);
        assert_eq!(multiply(rotation, translation)[3], [-2., 1., 3., 1.]);
    }
    #[test]
    fn source_json_transform_keeps_sub_float_precision() {
        let x = 0.123456789012345;
        assert_eq!(
            local_matrix(&json!({"translation":[x,0.,0.]})).unwrap()[3][0],
            x
        );
        assert_ne!((x as f32) as f64, x);
        assert!(local_matrix(&json!({"scale":[1.,0.,1.]})).is_err());
        assert!(local_matrix(&json!({"rotation":[0.,0.,0.,2.]})).is_err());
    }
    #[test]
    fn coverage_reference_and_count_changes_are_rejected() {
        let (path, _) = fixture();
        let original: Value =
            serde_json::from_slice(&std::fs::read(path.join("coverage.json")).unwrap()).unwrap();
        let mut coverage = original.clone();
        coverage["references"][0]["reference"] = json!("unexpected");
        std::fs::write(
            path.join("coverage.json"),
            serde_json::to_vec(&coverage).unwrap(),
        )
        .unwrap();
        assert!(
            adapt(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("reference set")
        );
        let mut coverage = original;
        coverage["unmodeled_reference_classification"]["test_contact; needs_source_pad_region_not_package_body"] =
            json!(2);
        std::fs::write(
            path.join("coverage.json"),
            serde_json::to_vec(&coverage).unwrap(),
        )
        .unwrap();
        assert!(
            adapt(&path)
                .err()
                .unwrap()
                .to_string()
                .contains("categories")
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}
