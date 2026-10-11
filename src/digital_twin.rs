//! Offline selection handoff for an existing, identity-labelled GLB candidate.
use crate::{EngineError, sha256_hex, strict_json};
use gltf::binary::{Glb, Header};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

const MAX_GLB: usize = 64 * 1024 * 1024;
const MAX_MAP: usize = 4 * 1024 * 1024;
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_NODES: usize = 100_000;
const MAX_COMPONENTS: usize = 10_000;

/// Constructor validates source bytes; selection does not trust an editable index.
pub struct ValidatedTwin {
    glb: Vec<u8>,
    document: Value,
    manifest: Value,
    map: Value,
    components: Vec<Value>,
    bindings: Vec<(usize, Vec<usize>, String)>,
    source_hashes: Value,
    missing_models: usize,
}

pub struct PreparedTwin {
    pub glb: Vec<u8>,
    pub manifest: Value,
    pub component_map: Value,
    pub index: Value,
    pub report: Value,
}

fn error(message: impl Into<String>) -> EngineError {
    EngineError::new(message)
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, EngineError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| error(format!("cannot read twin input: {e}")))?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(format!("cannot read twin input: {e}")))?;
    if bytes.len() > limit {
        return Err(error("twin input exceeds its byte limit"));
    }
    Ok(bytes)
}

fn parse(bytes: &[u8]) -> Result<Value, EngineError> {
    let text = std::str::from_utf8(bytes).map_err(|_| error("twin JSON is not UTF-8"))?;
    strict_json::parse(text).map_err(|e| error(format!("invalid twin JSON: {e}")))
}

fn text<'a>(value: &'a Value, key: &str, limit: usize) -> Result<&'a str, EngineError> {
    let s = value[key]
        .as_str()
        .ok_or_else(|| error(format!("missing string {key}")))?;
    if s.trim().is_empty() || s.len() > limit || s.chars().any(char::is_control) {
        return Err(error(format!("invalid identity/label {key}")));
    }
    Ok(s)
}

fn vector(value: &Value) -> Result<[f64; 3], EngineError> {
    let mut result = [0.0; 3];
    for (i, key) in ["x", "y", "z"].iter().enumerate() {
        let n = value[key]
            .as_f64()
            .ok_or_else(|| error("missing metric vector coordinate"))?;
        if !n.is_finite() || n.abs() > 10.0 {
            return Err(error(
                "metric coordinates must be finite and within 10 metres",
            ));
        }
        result[i] = n;
    }
    Ok(result)
}

impl ValidatedTwin {
    pub fn load(glb: &Path, map: &Path, manifest: &Path) -> Result<Self, EngineError> {
        Self::from_bytes(
            read_bounded(glb, MAX_GLB)?,
            &read_bounded(map, MAX_MAP)?,
            &read_bounded(manifest, MAX_MANIFEST)?,
        )
    }

    pub fn from_bytes(glb: Vec<u8>, map: &[u8], manifest: &[u8]) -> Result<Self, EngineError> {
        if glb.len() > MAX_GLB || map.len() > MAX_MAP || manifest.len() > MAX_MANIFEST {
            return Err(error("twin input exceeds its byte limit"));
        }
        // Check framing before the dependency parser can subtract header lengths.
        if glb.len() < 20
            || &glb[..4] != b"glTF"
            || u32::from_le_bytes(glb[4..8].try_into().unwrap()) != 2
            || u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize != glb.len()
        {
            return Err(error("invalid GLB v2 header or container length"));
        }
        let source_hashes = json!({
            "glb": sha256_hex(&glb), "componentMap": sha256_hex(map),
            "manifest": sha256_hex(manifest)
        });
        let manifest = parse(manifest)?;
        let map = parse(map)?;
        let version = text(&manifest, "schemaVersion", 32)?;
        let minor = version
            .strip_prefix("1.")
            .ok_or_else(|| error("unsupported twin schema"))?;
        if minor.is_empty() || !minor.bytes().all(|b| b.is_ascii_digit()) {
            return Err(error("unsupported twin schema"));
        }
        for (key, cap) in [
            ("assetId", 256),
            ("boardIdentity", 512),
            ("boardRevision", 128),
            ("assemblyContextRevision", 256),
        ] {
            text(&manifest, key, cap)?;
        }
        if manifest["units"] != "METRE"
            || manifest["axes"] != "RIGHT_HANDED_Y_UP"
            || manifest["componentNodePrefix"] != "ref:"
        {
            return Err(error(
                "twin must declare metres, right-handed Y-up and ref: selectors",
            ));
        }
        if manifest["glbBytes"].as_u64() != Some(glb.len() as u64)
            || text(&manifest, "glbSha256", 71)?
                .strip_prefix("sha256:")
                .unwrap_or(manifest["glbSha256"].as_str().unwrap())
                != source_hashes["glb"].as_str().unwrap()
        {
            return Err(error("twin manifest does not bind the GLB bytes/hash"));
        }
        vector(&manifest["boardOriginMetres"])?;
        let min = vector(&manifest["boundsMetres"]["minimum"])?;
        let max = vector(&manifest["boundsMetres"]["maximum"])?;
        if (0..3).any(|i| min[i] >= max[i]) {
            return Err(error("twin bounds must be strictly nondegenerate"));
        }
        if let Some(plane) = manifest
            .get("boardPlaneBoundsMetres")
            .filter(|v| !v.is_null())
        {
            let pmin = vector(&plane["minimum"])?;
            let pmax = vector(&plane["maximum"])?;
            if (0..3).any(|i| pmin[i] >= pmax[i] || pmin[i] < min[i] || pmax[i] > max[i]) {
                return Err(error(
                    "board-plane bounds must be nondegenerate and inside asset bounds",
                ));
            }
        }
        if let Some(corners) = manifest
            .get("boardPlaneCornerPointsMetres")
            .filter(|v| !v.is_null())
        {
            let corners = corners
                .as_array()
                .filter(|v| v.len() == 4)
                .ok_or_else(|| error("requires exactly four board-plane corners"))?;
            for corner in corners {
                let point = vector(corner)?;
                if (0..3).any(|i| point[i] < min[i] || point[i] > max[i]) {
                    return Err(error("board-plane corner is outside asset bounds"));
                }
            }
        }
        let opacity = manifest["defaultOpacity"]
            .as_f64()
            .ok_or_else(|| error("missing opacity"))?;
        if !(0.05..=0.85).contains(&opacity) {
            return Err(error("twin opacity outside App contract"));
        }
        if map["board_id"] != manifest["boardIdentity"]
            || map["board_revision"] != manifest["boardRevision"]
            || map["asset_id"] != manifest["assetId"]
        {
            return Err(error("component map and twin manifest identities differ"));
        }
        gltf::Gltf::from_slice(&glb).map_err(|e| error(format!("invalid glTF: {e}")))?;
        let container = Glb::from_slice(&glb).map_err(|e| error(format!("invalid GLB: {e}")))?;
        if container.header.version != 2
            || container.header.length as usize != glb.len()
            || container.bin.is_none()
        {
            return Err(error("requires a self-contained GLB v2"));
        }
        let document = parse(&container.json)?;
        for group in ["buffers", "images"] {
            if document[group]
                .as_array()
                .is_some_and(|items| items.iter().any(|v| v.get("uri").is_some()))
            {
                return Err(error("external or URI resources are unsupported"));
            }
        }
        let nodes = document["nodes"]
            .as_array()
            .ok_or_else(|| error("GLB has no nodes"))?;
        let meshes = document["meshes"]
            .as_array()
            .ok_or_else(|| error("GLB has no meshes"))?;
        let mut mesh_use_count = BTreeMap::new();
        for node in nodes {
            if let Some(mesh) = node["mesh"].as_u64() {
                *mesh_use_count.entry(mesh).or_insert(0usize) += 1;
            }
        }
        if nodes.len() > MAX_NODES {
            return Err(error("twin node limit exceeded"));
        }
        let scenes = document["scenes"]
            .as_array()
            .ok_or_else(|| error("GLB has no scene"))?;
        let scene = match document["scene"].as_u64() {
            Some(i) => usize::try_from(i).map_err(|_| error("invalid scene index"))?,
            None if scenes.len() == 1 => 0,
            _ => return Err(error("requires an explicit active scene")),
        };
        let roots = scenes
            .get(scene)
            .and_then(|v| v["nodes"].as_array())
            .ok_or_else(|| error("invalid active scene"))?;
        let mut stack = roots.clone();
        let mut reachable = BTreeSet::new();
        while let Some(index) = stack.pop() {
            let i = index
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .filter(|i| *i < nodes.len())
                .ok_or_else(|| error("invalid scene node index"))?;
            if !reachable.insert(i) {
                return Err(error("cycle/shared node in active scene"));
            }
            if let Some(children) = nodes[i]["children"].as_array() {
                stack.extend(children.iter().cloned());
            }
        }
        let mut selectors = BTreeMap::new();
        for (i, node) in nodes.iter().enumerate() {
            if let Some(name) = node["name"].as_str().filter(|n| n.starts_with("ref:")) {
                if !reachable.contains(&i) || selectors.insert(name.to_owned(), i).is_some() {
                    return Err(error("duplicate or unreachable component selector"));
                }
            }
        }
        let rows = map["components"]
            .as_array()
            .ok_or_else(|| error("component map has no components"))?;
        if rows.len() > MAX_COMPONENTS || map["component_count"].as_u64() != Some(rows.len() as u64)
        {
            return Err(error("invalid component map count"));
        }
        let mut references = BTreeSet::new();
        let mut expected = BTreeSet::new();
        let mut claimed_nodes = BTreeSet::new();
        let mut claimed_meshes = BTreeSet::new();
        let mut components = Vec::new();
        let mut bindings = Vec::new();
        let mut missing_models = 0;
        for row in rows {
            let reference = text(row, "reference", 256)?.to_owned();
            if !references.insert(reference.clone()) {
                return Err(error("duplicate component reference"));
            }
            let class = if row["component_class"].is_null() {
                None
            } else {
                Some(text(row, "component_class", 256)?.to_owned())
            };
            let needs_model = row["needs_3d_body"]
                .as_bool()
                .ok_or_else(|| error("missing needs_3d_body state"))?;
            let mut renderable = None;
            let mut renderable_nodes = Vec::new();
            let mut selector_root = None;
            if !row["glb_selector"].is_null() {
                let selector = text(row, "glb_selector", 260)?;
                if selector != format!("ref:{reference}") || !expected.insert(selector.to_owned()) {
                    return Err(error("map selector must match its reference"));
                }
                let root = *selectors
                    .get(selector)
                    .ok_or_else(|| error(format!("missing selector {selector}")))?;
                selector_root = Some(root);
                let mut todo = vec![root];
                let mut local = BTreeSet::new();
                let mut renderables = Vec::new();
                while let Some(i) = todo.pop() {
                    if !local.insert(i) || !claimed_nodes.insert(i) {
                        return Err(error("component subtrees overlap"));
                    }
                    if let Some(mesh) = nodes[i]["mesh"].as_u64() {
                        let mesh = usize::try_from(mesh).map_err(|_| error("invalid mesh"))?;
                        if mesh >= meshes.len() || !claimed_meshes.insert(mesh) {
                            return Err(error("component meshes overlap"));
                        }
                        if mesh_use_count.get(&(mesh as u64)) != Some(&1) {
                            return Err(error("component mesh is shared with another node"));
                        }
                        renderables.push(i);
                    }
                    if let Some(children) = nodes[i]["children"].as_array() {
                        for child in children {
                            let index = child
                                .as_u64()
                                .and_then(|v| usize::try_from(v).ok())
                                .filter(|i| *i < nodes.len())
                                .ok_or_else(|| error("invalid component child"))?;
                            todo.push(index);
                        }
                    }
                }
                if renderables.is_empty() {
                    return Err(error(
                        "component selector subtree contains no mesh-bearing nodes",
                    ));
                }
                renderables.sort_unstable();
                if renderables.len() == 1 {
                    renderable = Some(renderables[0]);
                }
                renderable_nodes = renderables.clone();
                bindings.push((root, renderables, reference.clone()));
            } else if needs_model {
                missing_models += 1;
            }
            components.push(json!({
                "reference": reference, "componentClassClaim": class,
                "selector": row["glb_selector"], "selectorNodeIndex": selector_root,
                "renderableNodeIndex": renderable, "modelQualityClaim": row["model_quality"],
                "renderableNodeIndices": renderable_nodes,
                "modelAvailable": !renderable_nodes.is_empty(), "needs3dBody": needs_model,
                "assemblyStateClaim": row["assembly_state"], "categoryClaim": row["category"]
            }));
        }
        if selectors.keys().cloned().collect::<BTreeSet<_>>() != expected
            || map["renderable_count"].as_u64() != Some(bindings.len() as u64)
        {
            return Err(error("GLB/map selector coverage differs"));
        }
        Ok(Self {
            glb,
            document,
            manifest,
            map,
            components,
            bindings,
            source_hashes,
            missing_models,
        })
    }

    pub fn index_json(&self) -> Value {
        let compatible = self
            .bindings
            .iter()
            .filter(|(root, meshes, _)| meshes.len() == 1 && meshes[0] == *root)
            .count();
        let component_mesh_nodes: BTreeSet<_> = self
            .bindings
            .iter()
            .flat_map(|(_, meshes, _)| meshes.iter().copied())
            .collect();
        let multi_mesh = self
            .bindings
            .iter()
            .filter(|(_, meshes, _)| meshes.len() > 1)
            .count();
        let unmapped: Vec<_> = self.document["nodes"].as_array().unwrap().iter().enumerate()
            .filter(|(index, node)| node.get("mesh").is_some() && !component_mesh_nodes.contains(index))
            .map(|(index, node)| json!({"nodeIndex":index,"name":node["name"],"meshIndex":node["mesh"]}))
            .collect();
        json!({
            "schema": "tokito.component-selection-index.v1",
            "boardIdentity": self.manifest["boardIdentity"], "boardRevision": self.manifest["boardRevision"],
            "assetId": self.manifest["assetId"], "assemblyContextRevisionClaim": self.manifest["assemblyContextRevision"],
            "sourceSha256": self.source_hashes, "componentCount": self.components.len(),
            "renderableCount": self.bindings.len(), "directRenderableSelectors": compatible,
            "renderableMeshNodeCount": component_mesh_nodes.len(), "multiMeshComponentCount": multi_mesh,
            "appNameFilterCompatible": compatible == self.bindings.len(), "missingRequiredModelCount": self.missing_models,
            "components": self.components, "physicalFitVerified": false, "arRegistrationVerified": false,
            "boardReferenceContextVerified": false, "positionalGuidanceAllowed": false
            ,"unmappedRenderableNodes": unmapped, "completeComponentCoverageVerified": false,
            "geometryBoundsComparedWithManifest": false
        })
    }

    pub fn select(
        &self,
        reference: Option<&str>,
        class: Option<&str>,
    ) -> Result<Value, EngineError> {
        let (key, query) = match (reference, class) {
            (Some(v), None) => ("reference", v),
            (None, Some(v)) => ("componentClassClaim", v),
            _ => {
                return Err(error(
                    "select exactly one reference or authored class label",
                ));
            }
        };
        if query.trim().is_empty() || query.len() > 256 || query.chars().any(char::is_control) {
            return Err(error("invalid selection query"));
        }
        let matches: Vec<_> = self
            .components
            .iter()
            .filter(|v| v[key].as_str() == Some(query))
            .cloned()
            .collect();
        let status = match matches.len() {
            0 => "no_match",
            1 if matches[0]["modelAvailable"] == true => "unique_candidate",
            1 => "model_unavailable",
            _ => "multiple_candidates",
        };
        Ok(json!({
            "schema": "tokito.component-selection.v1", "status": status,
            "query": {"field": key, "value": query}, "sourceSha256": self.source_hashes,
            "boardIdentity": self.manifest["boardIdentity"], "boardRevision": self.manifest["boardRevision"],
            "assemblyContextRevisionClaim": self.manifest["assemblyContextRevision"],
            "matches": matches, "positionalGuidanceAllowed": false,
            "limitations": ["Labels and physical revision/fit are source claims. Colour changes require App material handling; selection is not AR registration."]
        }))
    }

    pub fn prepare(mut self, allow_partial: bool) -> Result<PreparedTwin, EngineError> {
        if self.missing_models > 0 && !allow_partial {
            return Err(error(
                "required component models missing; use --allow-partial explicitly",
            ));
        }
        let source_hashes = self.source_hashes.clone();
        let original = Glb::from_slice(&self.glb).map_err(|e| error(e.to_string()))?;
        let mut changed = 0;
        for (root, renderables, reference) in &self.bindings {
            if renderables.len() == 1 && *root != renderables[0] {
                self.document["nodes"][*root]["name"] = json!(format!("placement:{reference}"));
                self.document["nodes"][renderables[0]]["name"] = json!(format!("ref:{reference}"));
                changed += 1;
            }
        }
        let bytes = Glb {
            header: Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&self.document).map_err(|e| error(e.to_string()))?),
            bin: original.bin.clone(),
        }
        .to_vec()
        .map_err(|e| error(e.to_string()))?;
        let digest = sha256_hex(&bytes);
        let old_id = text(&self.manifest, "assetId", 256)?.to_owned();
        let asset_id = format!("tokito-selectable-{}", &digest[..32]);
        self.manifest["assetId"] = json!(asset_id);
        self.manifest["glbSha256"] = json!(format!("sha256:{digest}"));
        self.manifest["glbBytes"] = json!(bytes.len());
        self.map["asset_id"] = self.manifest["assetId"].clone();
        let manifest_bytes =
            serde_json::to_vec_pretty(&self.manifest).map_err(|e| error(e.to_string()))?;
        let map_bytes = serde_json::to_vec_pretty(&self.map).map_err(|e| error(e.to_string()))?;
        let verified = Self::from_bytes(bytes.clone(), &map_bytes, &manifest_bytes)?;
        let index = verified.index_json();
        let report = json!({
            "schema": "tokito.digital-twin-preparation.v1", "status": "unregistered-reference-candidate",
            "sourceAssetId": old_id, "sourceSha256": source_hashes,
            "outputGlbSha256": digest, "renamedComponentSubtrees": changed,
            "binaryGeometryPayloadUnchanged": true, "nodeTransformsUnchanged": true,
            "componentCount": self.components.len(), "renderableCount": self.bindings.len(),
            "renderableMeshNodeCount": index["renderableMeshNodeCount"],
            "multiMeshComponentCount": index["multiMeshComponentCount"],
            "retainedGroupSelectors": index["multiMeshComponentCount"],
            "partialConsent": allow_partial, "missingRequiredModelCount": self.missing_models,
            "appNameFilterCompatible": index["appNameFilterCompatible"],
            "physicalFitVerified": false, "arRegistrationVerified": false,
            "boardReferenceContextVerified": false, "positionalGuidanceAllowed": false
        });
        Ok(PreparedTwin {
            glb: bytes,
            manifest: self.manifest,
            component_map: self.map,
            index,
            report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twin(nodes: Value, meshes: Value) -> Result<ValidatedTwin, EngineError> {
        let bin: Vec<u8> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let document = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],
            "bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"type":"VEC3","count":3,"min":[0.,0.,0.],"max":[1.,1.,0.]}],
            "meshes":meshes,"nodes":nodes,"scenes":[{"nodes":[0]}],"scene":0});
        let glb = Glb {
            header: Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&document).unwrap()),
            bin: Some(Cow::Owned(bin)),
        }
        .to_vec()
        .unwrap();
        let manifest = json!({"schemaVersion":"1.0","assetId":"fixture","boardIdentity":"fixture-board",
            "boardRevision":"reference","assemblyContextRevision":"fixture-1","units":"METRE",
            "axes":"RIGHT_HANDED_Y_UP","componentNodePrefix":"ref:","glbBytes":glb.len(),
            "glbSha256":sha256_hex(&glb),"boardOriginMetres":{"x":0.,"y":0.,"z":0.},
            "boundsMetres":{"minimum":{"x":-1.,"y":-1.,"z":-1.},"maximum":{"x":2.,"y":2.,"z":2.}},"defaultOpacity":0.5});
        let map = json!({"board_id":"fixture-board","board_revision":"reference","asset_id":"fixture",
            "component_count":1,"renderable_count":1,"components":[{"reference":"D1","component_class":"diode",
                "needs_3d_body":true,"glb_selector":"ref:D1","model_quality":"unverified-source"}]});
        ValidatedTwin::from_bytes(
            glb,
            &serde_json::to_vec(&map).unwrap(),
            &serde_json::to_vec(&manifest).unwrap(),
        )
    }

    fn meshes(count: usize) -> Value {
        Value::Array(
            (0..count)
                .map(|_| json!({"primitives":[{"attributes":{"POSITION":0}}]}))
                .collect(),
        )
    }

    #[test]
    fn multi_mesh_selection_retains_geometry_transforms_and_group_identity() {
        let original = twin(
            json!([{"name":"ref:D1","children":[1,2],"translation":[0.25,0.5,0.]},
            {"mesh":0,"translation":[0.01,0.,0.]},{"mesh":1,"rotation":[0.,0.,0.,1.]}]),
            meshes(2),
        )
        .unwrap();
        let original_doc = original.document.clone();
        let original_bin = Glb::from_slice(&original.glb)
            .unwrap()
            .bin
            .unwrap()
            .into_owned();
        let index = original.index_json();
        assert_eq!(
            index["components"][0]["renderableNodeIndices"],
            json!([1, 2])
        );
        assert!(index["components"][0]["renderableNodeIndex"].is_null());
        assert_eq!(index["renderableMeshNodeCount"], 2);
        assert_eq!(index["multiMeshComponentCount"], 1);
        assert_eq!(index["appNameFilterCompatible"], false);
        assert_eq!(index["unmappedRenderableNodes"], json!([]));
        assert_eq!(
            original.select(Some("D1"), None).unwrap()["status"],
            "unique_candidate"
        );
        let prepared = original.prepare(false).unwrap();
        let after = Glb::from_slice(&prepared.glb).unwrap();
        assert_eq!(after.bin.unwrap().as_ref(), original_bin);
        let after_doc: Value = serde_json::from_slice(&after.json).unwrap();
        assert_eq!(after_doc, original_doc);
        assert_eq!(prepared.report["retainedGroupSelectors"], 1);
        assert_eq!(prepared.report["renamedComponentSubtrees"], 0);
    }

    #[test]
    fn single_mesh_wrapper_still_normalizes_direct_selector() {
        let prepared = twin(
            json!([{"name":"ref:D1","children":[1],"translation":[0.25,0.5,0.]},
            {"mesh":0,"translation":[0.01,0.,0.]}]),
            meshes(1),
        )
        .unwrap()
        .prepare(false)
        .unwrap();
        assert_eq!(prepared.index["appNameFilterCompatible"], true);
        assert_eq!(prepared.index["components"][0]["selectorNodeIndex"], 1);
        assert_eq!(
            prepared.index["components"][0]["renderableNodeIndices"],
            json!([1])
        );
        assert_eq!(prepared.report["renamedComponentSubtrees"], 1);
    }

    #[test]
    fn empty_and_shared_mesh_groups_are_rejected() {
        assert!(
            twin(json!([{"name":"ref:D1"}]), meshes(1))
                .err()
                .unwrap()
                .to_string()
                .contains("no mesh")
        );
        assert!(
            twin(
                json!([{"name":"ref:D1","children":[1,2]}, {"mesh":0},{"mesh":0}]),
                meshes(1)
            )
            .err()
            .unwrap()
            .to_string()
            .contains("shared")
        );
    }
}
