//! Bounded local connectivity changes; never moves source or mesh vertices.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
type Uv = [f64; 2];
type P3 = [f64; 3];
type Edge = (usize, usize);
const MAX_PASSES: usize = 3;
const MAX_ATTEMPTS: usize = 128;
const MAX_SAMPLES: usize = 400_000;
fn edge(a: usize, b: usize) -> Edge {
    (a.min(b), a.max(b))
}
fn area(uv: &[Uv], t: [usize; 3]) -> f64 {
    let [a, b, c] = t.map(|i| uv[i]);
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}
fn distance(a: P3, b: P3) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}
fn numeric<const N: usize>(v: &Value) -> Result<[f64; N], String> {
    let row = v
        .as_array()
        .filter(|v| v.len() == N)
        .ok_or("edge_flip_invalid_numeric_array")?;
    let mut out = [0.; N];
    for (i, x) in row.iter().enumerate() {
        out[i] = x
            .as_f64()
            .filter(|x| x.is_finite())
            .ok_or("edge_flip_nonfinite_coordinate")?;
    }
    Ok(out)
}
fn validate_topology(triangles: &[[usize; 3]], boundary_count: usize) -> Result<(), String> {
    let boundary = (0..boundary_count)
        .map(|i| edge(i, (i + 1) % boundary_count))
        .collect::<BTreeSet<_>>();
    let mut edges: BTreeMap<Edge, (usize, isize)> = BTreeMap::new();
    for t in triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let e = edges.entry(edge(a, b)).or_default();
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    if boundary
        .iter()
        .any(|e| edges.get(e).is_none_or(|e| e.0 != 1))
        || edges.iter().any(|(e, (count, sense))| {
            if boundary.contains(e) {
                *count != 1
            } else {
                *count != 2 || *sense != 0
            }
        })
    {
        return Err("edge_flip_nonconforming_topology".into());
    }
    Ok(())
}
fn alignment<O>(t: [usize; 3], uv: &[Uv], points: &[P3], orientation: &mut O) -> Result<f64, String>
where
    O: FnMut([Uv; 3], [P3; 3]) -> Result<f64, String>,
{
    let a = orientation(t.map(|i| uv[i]), t.map(|i| points[i]))?;
    if !a.is_finite() || !(-1.000001..=1.000001).contains(&a) {
        return Err("edge_flip_invalid_normal_alignment".into());
    }
    Ok(a.clamp(-1., 1.))
}
fn triangle_error<F>(t: [usize; 3], uv: &[Uv], points: &[P3], sample: &mut F) -> Result<f64, String>
where
    F: FnMut(Uv) -> Result<P3, String>,
{
    let center = std::array::from_fn(|j| (uv[t[0]][j] + uv[t[1]][j] + uv[t[2]][j]) / 3.);
    let linear =
        std::array::from_fn(|j| (points[t[0]][j] + points[t[1]][j] + points[t[2]][j]) / 3.);
    let mut result = distance(sample(center)?, linear);
    for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
        let middle = std::array::from_fn(|j| (uv[a][j] + uv[b][j]) * 0.5);
        let linear = std::array::from_fn(|j| (points[a][j] + points[b][j]) * 0.5);
        result = result.max(distance(sample(middle)?, linear));
    }
    if !result.is_finite() {
        return Err("edge_flip_nonfinite_chord_error".into());
    }
    Ok(result)
}

pub(crate) fn improve<F, O>(candidate: &Value, evaluate: F, orientation: O) -> Result<Value, String>
where
    F: FnMut(Uv) -> Result<P3, String>,
    O: FnMut([Uv; 3], [P3; 3]) -> Result<f64, String>,
{
    improve_bounded(candidate, evaluate, orientation, MAX_SAMPLES)
}

fn improve_bounded<F, O>(
    candidate: &Value,
    mut evaluate: F,
    mut orientation: O,
    sample_limit: usize,
) -> Result<Value, String>
where
    F: FnMut(Uv) -> Result<P3, String>,
    O: FnMut([Uv; 3], [P3; 3]) -> Result<f64, String>,
{
    if sample_limit == 0 || sample_limit > MAX_SAMPLES {
        return Err("edge_flip_invalid_sample_budget".into());
    }
    if candidate["admitted"] != false
        || candidate["geometryQualified"] != false
        || candidate["exportsProduced"] != json!([])
    {
        return Err("edge_flip_requires_unqualified_research_candidate".into());
    }
    let vertices = candidate["uvVertices"]
        .as_array()
        .filter(|a| (3..=25_000).contains(&a.len()))
        .ok_or("edge_flip_vertex_limit")?;
    let uv = vertices
        .iter()
        .map(numeric::<2>)
        .collect::<Result<Vec<_>, _>>()?;
    let points = candidate["surfacePointsSourceUnits"]
        .as_array()
        .filter(|a| a.len() == uv.len())
        .ok_or("edge_flip_point_inventory_mismatch")?
        .iter()
        .map(numeric::<3>)
        .collect::<Result<Vec<_>, _>>()?;
    let rows = candidate["triangles"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 50_000)
        .ok_or("edge_flip_triangle_limit")?;
    let mut triangles = rows
        .iter()
        .map(|row| {
            let a = row
                .as_array()
                .filter(|r| r.len() == 3)
                .ok_or("edge_flip_invalid_triangle")?;
            let mut t = [0; 3];
            for (i, x) in a.iter().enumerate() {
                t[i] = x
                    .as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .filter(|v| *v < uv.len())
                    .ok_or("edge_flip_invalid_vertex_index")?;
            }
            let a = area(&uv, t);
            if !a.is_finite() || a <= 0. {
                return Err("edge_flip_degenerate_or_reversed_uv_triangle".into());
            }
            Ok(t)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let boundary_count = candidate["boundaryVertexCount"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n >= 3 && *n <= uv.len())
        .ok_or("edge_flip_invalid_boundary_inventory")?;
    let boundary = (0..boundary_count)
        .map(|i| edge(i, (i + 1) % boundary_count))
        .collect::<BTreeSet<_>>();
    validate_topology(&triangles, boundary_count)?;
    let mut normals = triangles
        .iter()
        .map(|t| alignment(*t, &uv, &points, &mut orientation))
        .collect::<Result<Vec<_>, _>>()?;
    let before_nonpositive = normals.iter().filter(|n| **n <= 0.).count();
    let mut samples = 0;
    let mut sample = |p: Uv| -> Result<P3, String> {
        samples += 1;
        if samples > sample_limit {
            return Err("edge_flip_sample_budget_exceeded".into());
        }
        let value = evaluate(p)?;
        if value.iter().any(|x| !x.is_finite()) {
            return Err("edge_flip_nonfinite_surface_point".into());
        }
        Ok(value)
    };
    let mut attempts = 0;
    let mut accepted = Vec::new();
    let mut passes = 0;
    for pass in 0..MAX_PASSES {
        passes = pass + 1;
        let mut incidence: BTreeMap<Edge, Vec<(usize, usize, usize, usize)>> = BTreeMap::new();
        for (i, t) in triangles.iter().enumerate() {
            for (a, b, c) in [(t[0], t[1], t[2]), (t[1], t[2], t[0]), (t[2], t[0], t[1])] {
                incidence.entry(edge(a, b)).or_default().push((i, a, b, c));
            }
        }
        let mut touched = BTreeSet::new();
        let mut introduced_edges = BTreeSet::new();
        let before = accepted.len();
        for (key, pair) in &incidence {
            if attempts >= MAX_ATTEMPTS {
                break;
            }
            if boundary.contains(key) || pair.len() != 2 {
                continue;
            }
            let (i, a, b, c) = pair[0];
            let (j, x, y, d) = pair[1];
            if touched.contains(&i) || touched.contains(&j) || normals[i].min(normals[j]) > 0. {
                continue;
            }
            if a != y || b != x {
                return Err("edge_flip_shared_edge_winding_conflict".into());
            }
            if c == d
                || incidence.contains_key(&edge(c, d))
                || introduced_edges.contains(&edge(c, d))
            {
                continue;
            }
            let replacement = [[c, a, d], [c, d, b]];
            if replacement.iter().any(|t| {
                let a = area(&uv, *t);
                !a.is_finite() || a <= 0.
            }) {
                continue;
            }
            attempts += 1;
            let new_normals = [
                alignment(replacement[0], &uv, &points, &mut orientation)?,
                alignment(replacement[1], &uv, &points, &mut orientation)?,
            ];
            if new_normals[0].min(new_normals[1]) <= 0.
                || new_normals[0].min(new_normals[1]) <= normals[i].min(normals[j])
            {
                continue;
            }
            let old_error = triangle_error(triangles[i], &uv, &points, &mut sample)?
                .max(triangle_error(triangles[j], &uv, &points, &mut sample)?);
            let new_error = triangle_error(replacement[0], &uv, &points, &mut sample)?
                .max(triangle_error(replacement[1], &uv, &points, &mut sample)?);
            if new_error > old_error {
                continue;
            }
            accepted.push(json!({"pass":pass,"triangleIndices":[i,j],"removedEdge":[a,b],"insertedEdge":[c,d],
                "oldTriangles":[triangles[i],triangles[j]],"newTriangles":replacement,
                "oldMinimumNormalAlignment":normals[i].min(normals[j]),"newMinimumNormalAlignment":new_normals[0].min(new_normals[1]),
                "oldMaximumSampledPairChordErrorSourceUnits":old_error,"newMaximumSampledPairChordErrorSourceUnits":new_error}));
            triangles[i] = replacement[0];
            triangles[j] = replacement[1];
            normals[i] = new_normals[0];
            normals[j] = new_normals[1];
            touched.insert(i);
            touched.insert(j);
            introduced_edges.insert(edge(c, d));
        }
        validate_topology(&triangles, boundary_count)?;
        if accepted.len() == before || attempts >= MAX_ATTEMPTS {
            break;
        }
    }
    let mut interior_error: f64 = 0.;
    let mut boundary_error: f64 = 0.;
    let mut seen = BTreeSet::new();
    for t in &triangles {
        let center = std::array::from_fn(|j| (uv[t[0]][j] + uv[t[1]][j] + uv[t[2]][j]) / 3.);
        let linear =
            std::array::from_fn(|j| (points[t[0]][j] + points[t[1]][j] + points[t[2]][j]) / 3.);
        let error = distance(sample(center)?, linear);
        if !error.is_finite() {
            return Err("edge_flip_nonfinite_chord_error".into());
        }
        interior_error = interior_error.max(error);
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let key = edge(a, b);
            if !seen.insert(key) {
                continue;
            }
            let error = distance(
                sample(std::array::from_fn(|j| (uv[a][j] + uv[b][j]) * 0.5))?,
                std::array::from_fn(|j| (points[a][j] + points[b][j]) * 0.5),
            );
            if !error.is_finite() {
                return Err("edge_flip_nonfinite_chord_error".into());
            }
            if boundary.contains(&key) {
                boundary_error = boundary_error.max(error);
            } else {
                interior_error = interior_error.max(error);
            }
        }
    }
    let target = candidate["minimumNormalAlignmentTarget"]
        .as_f64()
        .filter(|x| x.is_finite() && *x > 0. && *x <= 1.)
        .ok_or("edge_flip_normal_target_invalid")?;
    let mut report = candidate.clone();
    report["beforeInteriorRetriangulationQuality"] = candidate["quality"].clone();
    report["beforeInteriorRetriangulationTermination"] = candidate["termination"].clone();
    report["triangles"] = json!(triangles);
    report["quality"] = json!({"maximumSampledInteriorChordErrorSourceUnits":interior_error,"maximumSampledFixedBoundaryChordErrorSourceUnits":boundary_error,
        "minimumSampledNormalAlignment":normals.iter().copied().fold(1.,f64::min),"nonpositiveNormalCount":normals.iter().filter(|n|**n<=0.).count(),
        "normalThresholdFailureCount":normals.iter().filter(|n|**n<target).count()});
    let tolerance = candidate["chordToleranceSourceUnits"]
        .as_f64()
        .filter(|x| x.is_finite() && *x > 0.)
        .ok_or("edge_flip_chord_target_invalid")?;
    let targets_met = interior_error <= tolerance
        && boundary_error <= tolerance
        && normals.iter().all(|n| *n >= target);
    report["status"] = json!(if targets_met {
        "sampled_targets_met_unqualified"
    } else {
        "incomplete"
    });
    report["termination"] = json!(if targets_met {
        "interior_retriangulation_sampled_targets_met"
    } else {
        "interior_retriangulation_incomplete"
    });
    report["interiorRetriangulation"] = json!({"schema":"tokito.physical-interior-edge-flips.v1","passes":passes,"passLimit":MAX_PASSES,
        "attempts":attempts,"attemptLimit":MAX_ATTEMPTS,"acceptedFlipCount":accepted.len(),"acceptedFlips":accepted,
        "sampleEvaluationCount":samples,"sampleEvaluationLimit":sample_limit,"beforeNonpositiveNormalCount":before_nonpositive,
        "verticesMoved":0,"boundaryEdgesChanged":0,"allAcceptedPairNormalsPositive":true,"allAcceptedPairSampledChordErrorsNonIncreasing":true,
        "historyDescribesPreRetriangulationRefinement":true,"geometryQualified":false,"admitted":false});
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn surface(p: Uv) -> P3 {
        let [u, v] = p;
        [u, v, -9. * u * (1. - u) - 9. * v * (1. - v) + 3. * u * v]
    }
    fn normal(uv: [Uv; 3], p: [P3; 3]) -> Result<f64, String> {
        let u = (uv[0][0] + uv[1][0] + uv[2][0]) / 3.;
        let v = (uv[0][1] + uv[1][1] + uv[2][1]) / 3.;
        let expected = [9. - 18. * u - 3. * v, 9. - 18. * v - 3. * u, 1.];
        let a: [f64; 3] = std::array::from_fn(|i| p[1][i] - p[0][i]);
        let b: [f64; 3] = std::array::from_fn(|i| p[2][i] - p[0][i]);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = |x: P3| x[0].hypot(x[1]).hypot(x[2]);
        Ok(
            n.iter().zip(expected).map(|(a, b)| a * b).sum::<f64>()
                / (length(n) * length(expected)),
        )
    }
    fn fixture() -> Value {
        let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
        json!({"uvVertices":uv,"surfacePointsSourceUnits":uv.map(surface),"triangles":[[0,1,2],[0,2,3]],
            "boundaryVertexCount":4,"minimumNormalAlignmentTarget":0.99,"chordToleranceSourceUnits":0.01,
            "quality":{},"termination":"test_incomplete","admitted":false,"geometryQualified":false,"exportsProduced":[]})
    }
    #[test]
    fn actual_curved_surface_flip_improves_normals_and_sampled_error_without_moving_vertices() {
        let input = fixture();
        let r = improve(&input, |p| Ok(surface(p)), normal).unwrap();
        assert_eq!(r["uvVertices"], input["uvVertices"]);
        assert_eq!(
            r["surfacePointsSourceUnits"],
            input["surfacePointsSourceUnits"]
        );
        assert_eq!(r["interiorRetriangulation"]["acceptedFlipCount"], 1);
        assert_eq!(r["quality"]["nonpositiveNormalCount"], 0);
        let first = &r["interiorRetriangulation"]["acceptedFlips"][0];
        assert!(
            (first["oldMaximumSampledPairChordErrorSourceUnits"]
                .as_f64()
                .unwrap()
                - 5.25)
                .abs()
                < 1e-12
        );
        assert!(
            (first["newMaximumSampledPairChordErrorSourceUnits"]
                .as_f64()
                .unwrap()
                - 15. / 4.)
                .abs()
                < 1e-12
        );
        assert!(first["newMinimumNormalAlignment"].as_f64().unwrap() > 0.33);
        assert_eq!(r["status"], "incomplete");
        assert_eq!(r["admitted"], false);
        assert!(
            r["quality"]["normalThresholdFailureCount"]
                .as_u64()
                .unwrap()
                > 0
        );
    }
    #[test]
    fn no_improvement_and_nonfinite_or_resource_failures_never_publish_repaired_geometry() {
        let input = fixture();
        let r = improve(&input, |p| Ok(surface(p)), |_, _| Ok(1.)).unwrap();
        assert_eq!(r["triangles"], input["triangles"]);
        assert_eq!(r["interiorRetriangulation"]["acceptedFlipCount"], 0);
        assert!(improve(&input, |_| Ok([f64::NAN; 3]), normal).is_err());
        assert_eq!(
            improve(&input, |_| Err("bounded_test_refusal".into()), normal).unwrap_err(),
            "bounded_test_refusal"
        );
        assert!(improve(&input, |p| Ok(surface(p)), |_, _| Ok(f64::NAN)).is_err());
        let mut bad = input.clone();
        bad["triangles"] = json!([[0, 1, 2], [0, 1, 3]]);
        assert!(improve(&bad, |p| Ok(surface(p)), normal).is_err());
    }

    #[test]
    fn concavity_worsening_error_and_actual_budget_exhaustion_refuse_flips() {
        let input = fixture();
        let r = improve(
            &input,
            |p| {
                let mut v = surface(p);
                if (p[0] - 1. / 3.).abs() < 1e-12 && (p[1] - 1. / 3.).abs() < 1e-12 {
                    v[2] += 100.;
                }
                Ok(v)
            },
            normal,
        )
        .unwrap();
        assert_eq!(r["interiorRetriangulation"]["acceptedFlipCount"], 0);
        assert_eq!(r["triangles"], input["triangles"]);
        let uv = [[0., 0.], [1., 0.], [0.2, 0.2], [0., 1.]];
        let mut concave = input.clone();
        concave["uvVertices"] = json!(uv);
        concave["surfacePointsSourceUnits"] = json!(uv.map(surface));
        let r = improve(&concave, |p| Ok(surface(p)), |_, _| Ok(-1.)).unwrap();
        assert_eq!(r["interiorRetriangulation"]["attempts"], 0);
        assert_eq!(r["triangles"], concave["triangles"]);
        assert_eq!(
            improve_bounded(&input, |p| Ok(surface(p)), normal, 1).unwrap_err(),
            "edge_flip_sample_budget_exceeded"
        );
        assert_eq!(
            improve_bounded(&input, |p| Ok(surface(p)), normal, MAX_SAMPLES + 1).unwrap_err(),
            "edge_flip_invalid_sample_budget"
        );
    }
}
