//! Explicitly unqualified source-mesh assembly. This exports a selectable prototype,
//! never a physically admitted asset. Model-table placement is deliberately excluded.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
};

fn num(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite() && x.abs() < 1e6)
        .ok_or("invalid_finite_number".into())
}
fn vec3(v: &Value) -> Result<[f64; 3], String> {
    let a = v
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or("invalid_vector")?;
    Ok([num(&a[0])?, num(&a[1])?, num(&a[2])?])
}
fn load(path: &Path) -> Result<(Value, String), String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(96 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 96 * 1024 * 1024 {
        return Err("mesh_report_byte_limit".into());
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    Ok((
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
        digest,
    ))
}

fn rotate(p: [f64; 3], r: [f64; 3]) -> [f64; 3] {
    let (sx, cx) = r[0].to_radians().sin_cos();
    let (sy, cy) = r[1].to_radians().sin_cos();
    let (sz, cz) = r[2].to_radians().sin_cos();
    let p = [p[0], cx * p[1] - sx * p[2], sx * p[1] + cx * p[2]];
    let p = [cy * p[0] + sy * p[2], p[1], -sy * p[0] + cy * p[2]];
    [cz * p[0] - sz * p[1], sz * p[0] + cz * p[1], p[2]]
}
fn place(p: [f64; 3], pose: &Value, origin: [f64; 2], thickness: f64) -> Result<[f64; 3], String> {
    if pose["numericInputsResolved"] != true || pose["modelsTablePoseApplied"] != false {
        return Err("unresolved_pose_inputs".into());
    }
    let body = &pose["body"];
    let mut r = vec3(&body["rotation3dDegrees"])?;
    r[2] -= num(&body["rotation2dDegrees"])?;
    let bottom = match pose["component"]["side"].as_str() {
        Some("top") => false,
        Some("bottom") => true,
        _ => return Err("unsupported_side".into()),
    };
    if body["projection"] != if bottom { "bottom" } else { "top" } {
        return Err("body_projection_side_mismatch".into());
    }
    let mut p = rotate(p, r);
    if bottom {
        p[2] = -p[2];
    }
    p[0] += num(&body["positionMm"][0])? - origin[0];
    p[1] += num(&body["positionMm"][1])? - origin[1];
    let dz = num(&body["zOffsetMm"])?;
    p[2] += if bottom { -dz } else { thickness + dz };
    // Right-handed source XYZ -> right-handed glTF Y-up; lengths scaled once.
    Ok([p[0] * 0.001, p[2] * 0.001, -p[1] * 0.001])
}

fn restore_winding(mut triangle: [[f64; 3]; 3], bottom: bool) -> [[f64; 3]; 3] {
    if bottom {
        triangle.swap(1, 2);
    }
    triangle
}

#[derive(Default)]
struct Mesh {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    degenerate: usize,
    fallback_color_triangles: usize,
}
impl Mesh {
    fn triangle(&mut self, p: [[f64; 3]; 3], rgba: [f32; 4]) -> Result<(), String> {
        if self.positions.len() > 6_000_000 {
            return Err("assembly_triangle_budget".into());
        }
        if p.iter()
            .flatten()
            .any(|x| !x.is_finite() || x.abs() > 1000.)
        {
            return Err("nonfinite_or_unbounded_position".into());
        }
        let a = std::array::from_fn::<_, 3, _>(|i| p[1][i] - p[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| p[2][i] - p[0][i]);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let len = n.iter().map(|x| x * x).sum::<f64>().sqrt();
        if len < 1e-20 {
            self.degenerate += 1;
            return Ok(());
        }
        let n = n.map(|x| (x / len) as f32);
        for p in p {
            self.positions.push(p.map(|x| x as f32));
            self.normals.push(n);
            self.colors.push(rgba);
        }
        Ok(())
    }
}
struct Glb {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    nodes: Vec<Value>,
}
impl Glb {
    fn new() -> Self {
        Self {
            bin: vec![],
            views: vec![],
            accessors: vec![],
            meshes: vec![],
            nodes: vec![],
        }
    }
    fn attribute<const N: usize>(&mut self, data: &[[f32; N]], kind: &str, bounds: bool) -> usize {
        let start = self.bin.len();
        for row in data {
            for v in row {
                self.bin.extend(v.to_le_bytes());
            }
        }
        let view = self.views.len();
        self.views.push(
            json!({"buffer":0,"byteOffset":start,"byteLength":self.bin.len()-start,"target":34962}),
        );
        let mut acc =
            json!({"bufferView":view,"componentType":5126,"count":data.len(),"type":kind});
        if bounds {
            acc["min"] = json!((0..N)
                .map(|i| data.iter().map(|r| r[i]).fold(f32::INFINITY, f32::min))
                .collect::<Vec<_>>());
            acc["max"] = json!((0..N)
                .map(|i| data.iter().map(|r| r[i]).fold(f32::NEG_INFINITY, f32::max))
                .collect::<Vec<_>>());
        }
        let id = self.accessors.len();
        self.accessors.push(acc);
        id
    }
    fn mesh(&mut self, name: &str, m: &Mesh, extras: Value) -> Result<usize, String> {
        if m.positions.is_empty() {
            return Err("empty_component_mesh".into());
        }
        let p = self.attribute(&m.positions, "VEC3", true);
        let n = self.attribute(&m.normals, "VEC3", false);
        let c = self.attribute(&m.colors, "VEC4", false);
        let mesh = self.meshes.len();
        self.meshes.push(json!({"name":name,"primitives":[{"attributes":{"POSITION":p,"NORMAL":n,"COLOR_0":c},"material":0,"mode":4}]}));
        let node = self.nodes.len();
        self.nodes
            .push(json!({"name":name,"mesh":mesh,"extras":extras}));
        Ok(node)
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        let mut json=serde_json::to_vec(&json!({"asset":{"version":"2.0","generator":"Tokito original Rust Altium candidate assembly"},"scene":0,"scenes":[{"nodes":(0..self.nodes.len()).collect::<Vec<_>>()}],"nodes":self.nodes,"meshes":self.meshes,"materials":[{"name":"source STEP colour; physical reflectance unverified","pbrMetallicRoughness":{"baseColorFactor":[1,1,1,1],"metallicFactor":0.15,"roughnessFactor":0.45},"doubleSided":true}],"buffers":[{"byteLength":self.bin.len()}],"bufferViews":self.views,"accessors":self.accessors,"extras":{"geometryQualified":false,"poseVerified":false,"positionalGuidanceAllowed":false}})).map_err(|e|e.to_string())?;
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let total = 12 + 8 + json.len() + 8 + self.bin.len();
        let total = u32::try_from(total).map_err(|_| "glb_byte_limit")?;
        let mut out = Vec::new();
        out.extend(b"glTF");
        out.extend(2u32.to_le_bytes());
        out.extend(total.to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(b"JSON");
        out.extend(json);
        out.extend((self.bin.len() as u32).to_le_bytes());
        out.extend(b"BIN\0");
        out.extend(&self.bin);
        Ok(out)
    }
}

fn board_contour(board: &Value) -> Result<Vec<[f64; 2]>, String> {
    let records = board["contourRecords"]
        .as_array()
        .filter(|r| r.len() <= 1024)
        .ok_or("missing_contour")?;
    let mut points: Vec<[f64; 2]> = Vec::new();
    for record in records {
        let p = [num(&record["pointMm"][0])?, num(&record["pointMm"][1])?];
        points.push(p);
        if record["kind"] == "arc" {
            let c = [num(&record["centerMm"][0])?, num(&record["centerMm"][1])?];
            let radius = num(&record["radiusMm"])?;
            let a = (p[1] - c[1]).atan2(p[0] - c[0]);
            let start = num(&record["startAngleDegrees"])?.to_radians();
            let end = num(&record["endAngleDegrees"])?.to_radians();
            let delta = if (a.sin() - start.sin()).abs() + (a.cos() - start.cos()).abs() < 1e-5 {
                end - start
            } else {
                start - end
            };
            if delta.abs() > std::f64::consts::PI {
                return Err("unsupported_major_contour_arc".into());
            }
            for i in 1..=24 {
                let theta = a + delta * i as f64 / 24.;
                points.push([c[0] + radius * theta.cos(), c[1] + radius * theta.sin()]);
            }
        }
    }
    points.dedup_by(|a, b| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 1e-7);
    if points.len() > 2
        && (points[0][0] - points.last().unwrap()[0]).abs()
            + (points[0][1] - points.last().unwrap()[1]).abs()
            < 1e-7
    {
        points.pop();
    }
    // Only convex source contours are admitted to this prototype fan.
    let mut sign = 0.;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let c = points[(i + 2) % points.len()];
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() > 1e-7 {
            if sign != 0. && sign * cross < 0. {
                return Err("nonconvex_board_contour_unsupported".into());
            }
            sign = cross;
        }
    }
    if points.len() < 3 || sign == 0. {
        return Err("degenerate_board_contour".into());
    }
    if sign < 0. {
        points.reverse();
    }
    Ok(points)
}

fn board_mesh(board: &Value, origin: [f64; 2], thickness: f64) -> Result<Mesh, String> {
    let points = board_contour(board)?;
    let center = [
        points.iter().map(|p| p[0]).sum::<f64>() / points.len() as f64,
        points.iter().map(|p| p[1]).sum::<f64>() / points.len() as f64,
    ];
    let world = |p: [f64; 2], z: f64| {
        [
            (p[0] - origin[0]) * 0.001,
            z * 0.001,
            -(p[1] - origin[1]) * 0.001,
        ]
    };
    let mut mesh = Mesh::default();
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        for triangle in [
            [
                world(center, thickness),
                world(a, thickness),
                world(b, thickness),
            ],
            [world(center, 0.), world(b, 0.), world(a, 0.)],
            [world(a, 0.), world(b, 0.), world(b, thickness)],
            [world(a, 0.), world(b, thickness), world(a, thickness)],
        ] {
            mesh.triangle(triangle, [0.035, 0.055, 0.045, 1.])?;
        }
    }
    Ok(mesh)
}

fn drilled_board_mesh(
    board: &Value,
    origin: [f64; 2],
    thickness: f64,
    drills: &[Value],
) -> Result<(Mesh, Value), String> {
    if drills.len() > 16 || !thickness.is_finite() || thickness <= 0. {
        return Err("drilled_substrate_input_budget_or_thickness".into());
    }
    let outer: Vec<_> = board_contour(board)?
        .into_iter()
        .map(|p| [p[0] - origin[0], p[1] - origin[1]])
        .collect();
    let mut openings = Vec::new();
    let mut hashes = BTreeSet::new();
    let mut provenance = Vec::new();
    for report in drills {
        if report["schema"] != "tokito.excellon-source-inventory.v1" || report["lengthUnit"] != "mm"
        {
            return Err("unsupported_drill_inventory".into());
        }
        let hash = report["sourceSha256"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or("missing_drill_source_hash")?;
        if !hashes.insert(hash) {
            return Err("duplicate_drill_source".into());
        }
        let holes = report["holes"].as_array().ok_or("missing_drill_holes")?;
        let slots = report["slots"].as_array().ok_or("missing_drill_slots")?;
        if holes.len() + slots.len() > 512 || openings.len() + holes.len() + slots.len() > 512 {
            return Err("substrate_opening_budget".into());
        }
        let point = |value: &Value| -> Result<[f64; 2], String> {
            if value.as_array().map(Vec::len) != Some(2) {
                return Err("invalid_drill_point".into());
            }
            Ok([num(&value[0])?, num(&value[1])?])
        };
        for row in holes {
            let p = point(&row["positionMm"])?;
            openings.push((p, p, num(&row["diameterMm"])? / 2.));
        }
        for row in slots {
            openings.push((
                point(&row["startMm"])?,
                point(&row["endMm"])?,
                num(&row["diameterMm"])? / 2.,
            ));
        }
        provenance.push(report.clone());
    }
    if openings.is_empty() {
        return Err("empty_drill_geometry".into());
    }
    let profile = crate::substrate::triangulate(&outer, &openings, 0.001)?;
    let world = |p: [f64; 2], z: f64| [p[0] * 0.001, z * 0.001, -p[1] * 0.001];
    let mut mesh = Mesh::default();
    for tri in profile.indices.chunks_exact(3) {
        let mut p = [
            profile.points[tri[0] as usize],
            profile.points[tri[1] as usize],
            profile.points[tri[2] as usize],
        ];
        let cross =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0]);
        if cross < 0. {
            p.swap(1, 2);
        }
        mesh.triangle(p.map(|v| world(v, thickness)), [0.035, 0.055, 0.045, 1.])?;
        mesh.triangle(
            [world(p[0], 0.), world(p[2], 0.), world(p[1], 0.)],
            [0.035, 0.055, 0.045, 1.],
        )?;
    }
    for ring in &profile.rings {
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            mesh.triangle(
                [world(a, 0.), world(b, 0.), world(b, thickness)],
                [0.035, 0.055, 0.045, 1.],
            )?;
            mesh.triangle(
                [world(a, 0.), world(b, thickness), world(a, thickness)],
                [0.035, 0.055, 0.045, 1.],
            )?;
        }
    }
    let evidence = json!({"holesAndCutoutsIncluded":true,"openingCount":openings.len(),"drillCoordinateFrameDeclared":"board-origin-relative-xy-mm","sourceBoardFrameVerified":false,"physicalFitVerified":false,"maximumOpeningChordSagittaMm":profile.maximum_cutout_sagitta_mm,"requestedMaximumOpeningChordSagittaMm":0.001,"triangulatedMaterialAreaMm2":profile.triangulated_area_mm2,"sourcePolygonizedMaterialAreaMm2":profile.source_polygonized_material_area_mm2,"quantizedPolygonAreaDeltaMm2":profile.quantized_polygon_area_delta_mm2,"coordinateQuantizationMaximumMm":profile.coordinate_quantization_max_mm,"deliveryPrecision":profile.precision,"allSourceCutoutsIncluded":false,"outerContourSampling":"existing 24 subdivisions per source minor arc","polygonizedMaterialAreaMm2":profile.polygonized_area_mm2,"analyticCutoutAreaMm2":profile.analytic_cutout_area_mm2,"cutoutPolygonAreaErrorMm2":profile.cutout_polygon_error_mm2,"platedWallThicknessModelled":false,"sourceInventories":provenance});
    Ok((mesh, evidence))
}

pub fn export(source: &Value, mesh_dir: &Path, out: &Path) -> Result<Value, String> {
    export_with_drills(source, mesh_dir, out, &[])
}

pub fn export_with_drills(
    source: &Value,
    mesh_dir: &Path,
    out: &Path,
    drills: &[Value],
) -> Result<Value, String> {
    if out.exists() {
        return Err("output_directory_exists".into());
    }
    let board = &source["boardGeometryInputs"];
    if board["numericInputsResolved"] != true {
        return Err("missing_board_numeric_inputs".into());
    }
    let origin = [
        num(&board["sourceOriginMm"][0])?,
        num(&board["sourceOriginMm"][1])?,
    ];
    let thickness = num(&board["stack"]["copperAndCoreThicknessMm"])?;
    let mut glb = Glb::new();
    let (substrate_mesh, substrate_report) = if drills.is_empty() {
        (
            board_mesh(board, origin, thickness)?,
            json!({"holesAndCutoutsIncluded":false}),
        )
    } else {
        drilled_board_mesh(board, origin, thickness, drills)?
    };
    glb.mesh("board:source-contour", &substrate_mesh, json!({"holesAndCutoutsIncluded":!drills.is_empty(),"materialApproximate":true,"physicalThicknessVerified":false,"drillEvidence":substrate_report}))?;
    let components = source["candidateAssociations"]["components"]
        .as_array()
        .filter(|c| c.len() <= 2048)
        .ok_or("missing_component_candidates")?;
    let mut cache: BTreeMap<u64, (Value, String)> = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut refs = BTreeSet::new();
    let mut rows = Vec::new();
    let mut body_count = 0;
    let mut triangles = 0;
    let mut cached_bytes = 0u64;
    for component in components {
        let id = component["sourceIdentity"]["uniqueId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("missing_component_unique_id")?;
        let reference = component["sourceIdentity"]["reference"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("missing_reference")?;
        if !ids.insert(id) || !refs.insert(reference) {
            return Err("duplicate_component_identity".into());
        }
        let selector = format!("ref:{reference}");
        let mut mesh = Mesh::default();
        let mut body_reports = Vec::new();
        for body in component["bodyCandidates"]
            .as_array()
            .ok_or("missing_bodies")?
        {
            if body["embeddedByteIdentityAgrees"] != true {
                return Err("ambiguous_model_geometry".into());
            }
            let model = body["modelCandidates"]
                .as_array()
                .and_then(|m| m.first())
                .ok_or("missing_model")?;
            let ordinal = model["modelOrdinal"]
                .as_u64()
                .filter(|n| *n < 2048)
                .ok_or("invalid_model_ordinal")?;
            if !cache.contains_key(&ordinal) {
                if cache.len() >= 64 {
                    return Err("cached_model_count_limit".into());
                }
                let path = mesh_dir.join(format!("{ordinal}.mesh.json"));
                cached_bytes += std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
                if cached_bytes > 256 * 1024 * 1024 {
                    return Err("aggregate_mesh_report_byte_limit".into());
                }
                cache.insert(ordinal, load(&path)?);
            }
            let (report, report_hash) = &cache[&ordinal];
            if report["sourceSha256"] != model["embeddedStepEvidence"]["decompressedSha256"] {
                return Err("step_report_source_hash_mismatch".into());
            }
            let candidate = &report["candidateMeshData"];
            if candidate["sourceSha256"] != model["embeddedStepEvidence"]["decompressedSha256"] {
                return Err("nested_mesh_source_hash_mismatch".into());
            }
            if candidate["schema"] != "tokito.step-positioned-candidate-mesh.v1"
                || !candidate["occurrences"].is_array()
                || candidate["lengthUnit"] != "mm"
                || report["assemblyOccurrenceWorld"]["transformCompositionResolved"] != true
            {
                return Err("unresolved_mesh_units_or_occurrences".into());
            }
            let before = mesh.positions.len();
            for occurrence in candidate["occurrences"]
                .as_array()
                .ok_or("missing_occurrences")?
            {
                for shape in occurrence["shapes"].as_array().ok_or("missing_shapes")? {
                    let vertices = shape["verticesMm"]
                        .as_array()
                        .filter(|v| v.len() <= 1_000_000)
                        .ok_or("invalid_vertices")?;
                    let tris = shape["triangles"]
                        .as_array()
                        .filter(|t| t.len() <= 2_000_000)
                        .ok_or("invalid_triangles")?;
                    for (index, tri) in tris.iter().enumerate() {
                        let indices = tri
                            .as_array()
                            .filter(|v| v.len() == 3)
                            .ok_or("invalid_triangle")?;
                        let mut p = [[0.; 3]; 3];
                        for corner in 0..3 {
                            let vertex = indices[corner]
                                .as_u64()
                                .and_then(|i| usize::try_from(i).ok())
                                .and_then(|i| vertices.get(i))
                                .ok_or("triangle_index_out_of_range")?;
                            p[corner] =
                                place(vec3(vertex)?, &body["sourcePoseInputs"], origin, thickness)?;
                        }
                        let rgba = shape["sourceTriangleRgba"]
                            .get(index)
                            .filter(|c| !c.is_null())
                            .or_else(|| shape.get("sourceItemRgba"))
                            .filter(|c| !c.is_null());
                        let fallback = json!([0.55, 0.55, 0.55, 1.0]);
                        if rgba.is_none() {
                            mesh.fallback_color_triangles += 1;
                        }
                        let rgba = rgba.unwrap_or(&fallback);
                        let rgba = rgba
                            .as_array()
                            .filter(|a| a.len() == 4)
                            .ok_or("invalid_color")?;
                        let mut color = [0.; 4];
                        for i in 0..4 {
                            let value = num(&rgba[i])?;
                            if !(0.0..=1.0).contains(&value) {
                                return Err("invalid_color_range".into());
                            }
                            color[i] = value as f32;
                        }
                        // A bottom-side Z reflection reverses handedness; restore
                        // winding before deriving the outward flat normal.
                        let p = restore_winding(
                            p,
                            body["sourcePoseInputs"]["component"]["side"] == "bottom",
                        );
                        mesh.triangle(p, color)?;
                    }
                }
            }
            body_count += 1;
            body_reports.push(json!({"bodyOrdinal":body["bodyOrdinal"],"modelOrdinal":ordinal,"stepSha256":report["sourceSha256"],"meshReportSha256":report_hash,"triangles":(mesh.positions.len()-before)/3,"sourcePoseInputs":body["sourcePoseInputs"],"geometryQualified":false,"poseVerified":false}));
        }
        let count = mesh.positions.len() / 3;
        triangles += count;
        if triangles > 2_000_000 {
            return Err("aggregate_assembly_triangle_limit".into());
        }
        let node=glb.mesh(&selector,&mesh,json!({"componentId":id,"reference":reference,"bodyCount":body_reports.len(),"poseVerified":false}))?;
        rows.push(json!({"component_id":id,"reference":reference,"glb_selector":selector,"glb_node_index":node,"identity_verified":false,"model_quality":"source_step_candidate_pose_surface_unqualified","needs_3d_body":true,"body_count":body_reports.len(),"triangle_count":count,"degenerate_triangles_omitted":mesh.degenerate,"neutral_fallback_color_triangle_count":mesh.fallback_color_triangles,"sourceBodies":body_reports}));
    }
    let bytes = glb.bytes()?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let minimum = std::array::from_fn::<_, 3, _>(|i| {
        glb.accessors
            .iter()
            .filter_map(|a| a["min"][i].as_f64())
            .fold(f64::INFINITY, f64::min)
    });
    let maximum = std::array::from_fn::<_, 3, _>(|i| {
        glb.accessors
            .iter()
            .filter_map(|a| a["max"][i].as_f64())
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let vector = |p: [f64; 3]| json!({"x":p[0],"y":p[1],"z":p[2]});
    let asset_id = format!("tokito-esp8266-{}", &digest[..32]);
    let revision = "altium-body-source-candidate-v1";
    let manifest = json!({"schemaVersion":"1.0","assetId":asset_id,"boardIdentity":"esp8266-nodemcu-altium-reference","boardRevision":source["sourceSha256"].as_str().map(|s|&s[..12]),"assemblyContextRevision":revision,"units":"METRE","axes":"RIGHT_HANDED_Y_UP","boardOriginMetres":{"x":0,"y":0,"z":0},"boundsMetres":{"minimum":vector(minimum),"maximum":vector(maximum)},"glbBytes":bytes.len(),"glbSha256":format!("sha256:{digest}"),"componentNodePrefix":"ref:","defaultOpacity":0.42,"physicalGeometryVerified":false,"arRegistrationVerified":false,"releaseEligible":false});
    let map = json!({"schema":"tokito.component-map.v1","asset_id":asset_id,"board_id":"esp8266-nodemcu-altium-reference","board_revision":source["sourceSha256"].as_str().map(|s|&s[..12]),"assemblyContextRevision":revision,"component_node_prefix":"ref:","component_count":rows.len(),"renderable_count":rows.len(),"selectable_component_count":rows.len(),"components":rows,"physical_fit_verified":false,"calibration_eligible":false,"positional_guidance_allowed":false});
    let report = json!({"schema":"tokito.altium-prototype-assembly.v1","sourceSha256":source["sourceSha256"],"glbSha256":digest,"glbBytes":bytes.len(),"componentCount":components.len(),"bodyCount":body_count,"componentTriangles":triangles,"selectedModelCount":cache.len(),"prototypeExported":true,"assemblyAdmitted":false,"geometryQualified":false,"poseVerified":false,"positionalGuidanceAllowed":false,"boardThicknessMm":thickness,"boardOriginMm":origin,"boardTopZMm":thickness,"boardBottomZMm":0,"boardZDatumAssumed":true,"holesAndCutoutsIncluded":false,"modelsTablePoseApplied":false,"transformMethod":"body absolute XY minus board source origin; Rx then Ry then Rz(body.rz-body.angle); bottom z reflection; top copper/core surface or bottom zero candidate datum; glTF [x,z,-y] metres","diagnostics":["source_surface_fidelity_unqualified","source_body_component_index_semantics_unqualified","board_z_datum_and_bottom_projection_unqualified","pcb_pads_drills_silkscreen_not_exported","physical_photo_variant_unverified","independent_evaluated_surface_comparison_pending","step_occurrence_colour_override_application_unresolved"]});
    let mut report = report;
    if !drills.is_empty() {
        report["holesAndCutoutsIncluded"] = json!(true);
        report["drillEvidence"] = substrate_report;
        if let Some(diagnostics) = report["diagnostics"].as_array_mut() {
            for diagnostic in diagnostics.iter_mut() {
                if *diagnostic == "pcb_pads_drills_silkscreen_not_exported" {
                    *diagnostic =
                        json!("pcb_pads_silkscreen_and_plated_wall_thickness_not_exported");
                }
            }
            diagnostics.push(json!(
                "drill_board_relative_frame_explicitly_declared_not_physically_verified"
            ));
        }
    }
    // Exclusive directory/file creation also closes the exists-check race.
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    let write_new = |name: &str, bytes: &[u8]| -> Result<(), String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(name))
            .and_then(|mut file| file.write_all(bytes))
            .map_err(|e| e.to_string())
    };
    write_new("active.digital-twin.glb", &bytes)?;
    for (name, value) in [
        ("active.digital-twin.json", manifest),
        ("component-map.json", map),
        ("prepare-report.json", report.clone()),
    ] {
        write_new(
            name,
            &serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        )?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(side: &str) -> Value {
        json!({"numericInputsResolved":true,"modelsTablePoseApplied":false,"component":{"side":side,"rotationDegrees":123},"body":{"positionMm":[10,20],"rotation3dDegrees":[0,0,90],"rotation2dDegrees":0,"zOffsetMm":0.5,"projection":side}})
    }
    #[test]
    fn top_and_bottom_scale_once_and_ignore_component_angle() {
        let top = place([1., 0., 2.], &pose("top"), [10., 20.], 0.4).unwrap();
        assert!((top[2] + 0.001).abs() < 1e-12);
        assert!((top[1] - 0.0029).abs() < 1e-12);
        let bottom = place([1., 0., 2.], &pose("bottom"), [10., 20.], 0.4).unwrap();
        assert!((bottom[1] + 0.0025).abs() < 1e-12);
    }
    #[test]
    fn mismatch_and_double_model_pose_refuse() {
        let mut p = pose("top");
        p["body"]["projection"] = json!("bottom");
        assert!(place([0.; 3], &p, [0.; 2], 0.4).is_err());
        p["body"]["projection"] = json!("top");
        p["modelsTablePoseApplied"] = json!(true);
        assert!(place([0.; 3], &p, [0.; 2], 0.4).is_err());
    }
    #[test]
    fn finite_nonempty_triangles_have_unit_normals() {
        let mut mesh = Mesh::default();
        mesh.triangle([[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]], [1.; 4])
            .unwrap();
        assert_eq!(mesh.normals[0], [0., 0., 1.]);
        mesh.triangle([[0.; 3]; 3], [1.; 4]).unwrap();
        assert_eq!(mesh.degenerate, 1);
        assert_eq!(mesh.positions.len(), 3);
    }
    #[test]
    fn clockwise_board_has_upward_top_and_downward_bottom_normals() {
        let board = json!({"contourRecords":[
            {"kind":"line","pointMm":[0,0]},
            {"kind":"line","pointMm":[0,1]},
            {"kind":"line","pointMm":[1,1]},
            {"kind":"line","pointMm":[1,0]},
            {"kind":"line","pointMm":[0,0]}]});
        let mesh = board_mesh(&board, [0.; 2], 0.4).unwrap();
        assert_eq!(mesh.normals[0], [0., 1., 0.]);
        assert_eq!(mesh.normals[3], [0., -1., 0.]);
    }
    #[test]
    fn bottom_reflection_restores_outward_triangle_normal() {
        let mut p = pose("bottom");
        p["body"]["rotation3dDegrees"] = json!([0, 0, 0]);
        let triangle = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]]
            .map(|v| place(v, &p, [10., 20.], 0.4).unwrap());
        let mut mesh = Mesh::default();
        mesh.triangle(restore_winding(triangle, true), [1.; 4])
            .unwrap();
        assert_eq!(mesh.normals[0], [0., -1., 0.]);
    }
}
