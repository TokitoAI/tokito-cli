//! Prevent a partial split from turning passing parents into failing transitions.
use crate::priority_refinement::{Edge, Selection};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
type Uv = [f64; 2];
type P3 = [f64; 3];
const MAX_PASSES: usize = 16;
const MAX_CHILD_NORMALS: usize = 200_000;

pub(crate) struct Mesh<'a> {
    pub uv: &'a [Uv],
    pub points: &'a [P3],
    pub triangles: &'a [[usize; 3]],
    pub midpoint_points: &'a BTreeMap<Edge, P3>,
    pub parent_normals: &'a [f64],
    pub target: f64,
}
pub(crate) struct Checked {
    pub selection: Selection,
    pub alternate_parents: BTreeSet<usize>,
    pub complete: bool,
}
fn edge(a: usize, b: usize) -> Edge {
    (a.min(b), a.max(b))
}
fn safe(normals: &[f64], parent: f64, target: f64) -> bool {
    normals.iter().all(|n| *n > 0.)
        && if parent >= target {
            normals.iter().all(|n| *n >= target)
        } else {
            true
        }
}

pub(crate) fn check<O, R>(
    m: Mesh<'_>,
    mut selection: Selection,
    orientation: &mut O,
    remaining: &mut R,
) -> Result<Checked, String>
where
    O: FnMut([Uv; 3], [P3; 3]) -> Result<f64, String>,
    R: FnMut() -> Result<usize, String>,
{
    if m.parent_normals.len() != m.triangles.len() {
        return Err("priority_quality_parent_inventory_mismatch".into());
    }
    let initial_count = selection.edges.len();
    let initial_groups = selection.groups.len();
    let mut cache = BTreeMap::<(usize, u8, bool), Vec<f64>>::new();
    let mut evaluated = 0;
    let mut passes = 0;
    let mut alternatives = BTreeSet::new();
    let mut complete = false;
    let mut budget_stopped = false;
    'passes: for pass in 0..MAX_PASSES {
        passes = pass + 1;
        alternatives.clear();
        let n = selection.edges.len();
        let reserve = 5 * m.triangles.len() + m.midpoint_points.len() + 13 * n;
        let mut unsafe_edges = BTreeSet::new();
        for (index, t) in m.triangles.iter().enumerate() {
            let keys = [edge(t[0], t[1]), edge(t[1], t[2]), edge(t[2], t[0])];
            let mask = (0..3).fold(0u8, |mask, i| {
                mask | if selection.edges.contains(&keys[i]) {
                    1 << i
                } else {
                    0
                }
            });
            if mask == 0 {
                continue;
            }
            let local_uv: [Uv; 6] = std::array::from_fn(|i| {
                if i < 3 {
                    m.uv[t[i]]
                } else {
                    let (a, b) = keys[i - 3];
                    std::array::from_fn(|j| (m.uv[a][j] + m.uv[b][j]) * 0.5)
                }
            });
            let local_points: [P3; 6] = std::array::from_fn(|i| {
                if i < 3 {
                    m.points[t[i]]
                } else {
                    m.midpoint_points[&keys[i - 3]]
                }
            });
            let midpoints = (0..3)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| (edge(i, (i + 1) % 3), i + 3))
                .collect::<BTreeMap<_, _>>();
            let mut acceptable = false;
            for alternate in [false, true] {
                let children = if alternate {
                    let Some(children) =
                        crate::physical_surface_refinement::split_triangle_alternative(
                            [0, 1, 2],
                            &midpoints,
                        )
                    else {
                        continue;
                    };
                    children
                } else {
                    crate::physical_surface_refinement::split_triangle([0, 1, 2], &midpoints)
                };
                let key = (index, mask, alternate);
                if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(key) {
                    let mut normals = Vec::new();
                    for child in children {
                        if evaluated >= MAX_CHILD_NORMALS || remaining()? < reserve + 4 {
                            budget_stopped = true;
                            break 'passes;
                        }
                        let normal = orientation(
                            child.map(|i| local_uv[i]),
                            child.map(|i| local_points[i]),
                        )?;
                        if !normal.is_finite() || !(-1.000001..=1.000001).contains(&normal) {
                            return Err("priority_quality_invalid_child_normal".into());
                        }
                        normals.push(normal.clamp(-1., 1.));
                        evaluated += 1;
                    }
                    entry.insert(normals);
                }
                if safe(&cache[&key], m.parent_normals[index], m.target) {
                    if alternate {
                        alternatives.insert(index);
                    }
                    acceptable = true;
                    break;
                }
            }
            if !acceptable {
                for key in keys {
                    if selection.edges.contains(&key) {
                        unsafe_edges.insert(key);
                    }
                }
            }
        }
        if unsafe_edges.is_empty() {
            complete = true;
            break;
        }
        selection
            .groups
            .retain(|g| !g.edges.iter().any(|e| unsafe_edges.contains(e)));
        let retained = selection
            .groups
            .iter()
            .flat_map(|g| g.edges.iter().copied())
            .collect::<BTreeSet<_>>();
        if retained.len() >= selection.edges.len() {
            return Err("priority_quality_pruning_did_not_progress".into());
        }
        selection.edges = retained;
    }
    if !complete {
        selection.edges.clear();
        selection.groups.clear();
        alternatives.clear();
    }
    let n = selection.edges.len();
    let satisfied = selection
        .obligations
        .iter()
        .filter(|o| !o.edges.is_empty() && o.edges.iter().all(|e| selection.edges.contains(e)))
        .count();
    selection.report["selectedEdges"] = json!(selection
        .edges
        .iter()
        .map(|(a, b)| [*a, *b])
        .collect::<Vec<_>>());
    selection.report["selectedEdgeCount"] = json!(n);
    selection.report["deferredEdgeCount"] = json!(
        selection.report["markedEdgeCount"]
            .as_u64()
            .ok_or("priority_quality_marked_count_missing")?
            - n as u64
    );
    selection.report["acceptedObligationCount"] = json!(selection.groups.len());
    selection.report["alreadySatisfiedObligationCount"] = json!(satisfied - selection.groups.len());
    selection.report["deferredObligationCount"] = json!(selection.obligations.len() - satisfied);
    selection.report["acceptedNormalObligationCount"] =
        json!(selection.groups.iter().filter(|g| g.normal_failure).count());
    selection.report["deferredNormalObligationCount"] = json!(selection
        .obligations
        .iter()
        .filter(|o| o.normal_failure
            && (o.edges.is_empty() || !o.edges.iter().all(|e| selection.edges.contains(e))))
        .count());
    selection.report["reservedNextChordSamples"] = json!(if n > 0 {
        m.triangles.len() + m.midpoint_points.len() + 5 * n
    } else {
        0
    });
    selection.report["reservedNextSurfaceEvaluations"] = json!(if n > 0 {
        5 * m.triangles.len() + m.midpoint_points.len() + 13 * n
    } else {
        0
    });
    selection.report["expectedNextVertices"] = json!(m.uv.len() + n);
    selection.report["expectedNextTriangles"] = json!(m.triangles.len() + 2 * n);
    selection.report["expectedNextEdges"] = json!(m.midpoint_points.len() + 3 * n);
    selection.report["transitionQualityPreflight"] = json!({"complete":complete,"passes":passes,"passLimit":MAX_PASSES,
        "initialSelectedEdgeCount":initial_count,"initialAcceptedGroupCount":initial_groups,
        "prunedGroupCount":initial_groups-selection.groups.len(),"childNormalEvaluationCount":evaluated,
        "childNormalEvaluationLimit":MAX_CHILD_NORMALS,"budgetStopped":budget_stopped,
        "surfaceEvaluationsRemainingAfterPreflight":remaining()?,"alternateParents":alternatives.iter().copied().collect::<Vec<_>>(),
        "protectsPassingParentsFromNewNormalFailures":true,"boundaryEdgesChanged":0,
        "geometryQualified":false,"admitted":false});
    Ok(Checked {
        selection,
        alternate_parents: alternatives,
        complete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn point(p: Uv, c: [f64; 3]) -> P3 {
        [
            p[0],
            p[1],
            c[0] * p[0] * p[0] + c[1] * p[0] * p[1] + c[2] * p[1] * p[1],
        ]
    }
    fn normal(uv: [Uv; 3], p: [P3; 3], c: [f64; 3]) -> f64 {
        let u = (uv[0][0] + uv[1][0] + uv[2][0]) / 3.;
        let v = (uv[0][1] + uv[1][1] + uv[2][1]) / 3.;
        let expected = [-2. * c[0] * u - c[1] * v, -c[1] * u - 2. * c[2] * v, 1.];
        let a: P3 = std::array::from_fn(|i| p[1][i] - p[0][i]);
        let b: P3 = std::array::from_fn(|i| p[2][i] - p[0][i]);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let norm = |p: P3| p[0].hypot(p[1]).hypot(p[2]);
        n.iter().zip(expected).map(|(a, b)| a * b).sum::<f64>() / (norm(n) * norm(expected))
    }
    fn run(c: [f64; 3], selected: &[Edge], remaining: usize) -> Checked {
        let uv = [[0., 0.], [1., 0.], [0., 1.]];
        let points = uv.map(|p| point(p, c));
        let triangles = [[0, 1, 2]];
        let midpoints = [(0, 1), (1, 2), (0, 2)]
            .into_iter()
            .map(|e| {
                let uv = std::array::from_fn(|i| (uv[e.0][i] + uv[e.1][i]) * 0.5);
                (e, point(uv, c))
            })
            .collect();
        let parents = [normal(uv, points, c)];
        let target = 5f64.to_radians().cos();
        assert!(parents[0] >= target);
        let marked = selected.iter().copied().collect();
        let selection = crate::priority_refinement::select(
            vec![crate::priority_refinement::Obligation {
                normal_failure: false,
                severity: 2.,
                identity: (0, 0, 0),
                edges: selected.to_vec(),
            }],
            &marked,
            crate::priority_refinement::Budget {
                vertices: 3,
                triangles: 1,
                edges: 3,
                chord_remaining: 400000,
                surface_remaining: 1000000,
            },
        )
        .unwrap();
        check(
            Mesh {
                uv: &uv,
                points: &points,
                triangles: &triangles,
                midpoint_points: &midpoints,
                parent_normals: &parents,
                target,
            },
            selection,
            &mut |u, p| Ok(normal(u, p, c)),
            &mut || Ok(remaining),
        )
        .unwrap()
    }
    #[test]
    fn analytic_two_edge_alternative_protects_a_previously_passing_parent() {
        let r = run(
            [
                -0.010553246231700064,
                0.0821737325402987,
                0.1515698908165357,
            ],
            &[(0, 1), (1, 2)],
            1000000,
        );
        assert!(r.complete);
        assert_eq!(r.selection.edges.len(), 2);
        assert_eq!(r.alternate_parents, BTreeSet::from([0]));
        assert_eq!(
            r.selection.report["transitionQualityPreflight"]["prunedGroupCount"],
            0
        );
    }
    #[test]
    fn unsafe_transition_prunes_its_group_and_one_call_short_refuses_preflight() {
        let r = run([1., 1., 1.], &[(0, 1)], 1000000);
        assert!(r.complete);
        assert!(r.selection.edges.is_empty());
        assert_eq!(
            r.selection.report["transitionQualityPreflight"]["passes"],
            2
        );
        assert_eq!(
            r.selection.report["transitionQualityPreflight"]["prunedGroupCount"],
            1
        );
        let r = run([1., 1., 1.], &[(0, 1)], 24); // Next whole pass=21, first orientation needs4.
        assert!(!r.complete);
        assert!(r.selection.edges.is_empty());
        assert_eq!(
            r.selection.report["transitionQualityPreflight"]["childNormalEvaluationCount"],
            0
        );
    }

    #[test]
    fn removing_a_group_rechecks_the_changed_neighbor_mask() {
        let uv = [[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
        let height = |u: f64, v: f64| {
            10000.
                * u
                * (1. - u)
                * v
                * (1. - v)
                * (u + v - 1.).powi(2)
                * (v - 0.5).powi(2)
                * (v - 5. / 6.).powi(2)
        };
        let point = |p: Uv| [p[0], p[1], height(p[0], p[1])];
        let points = uv.map(point);
        let triangles = [[0, 1, 2], [0, 2, 3]];
        let edges = [(0, 1), (1, 2), (0, 2), (2, 3), (0, 3)];
        let midpoints = edges
            .into_iter()
            .map(|e| {
                let p = std::array::from_fn(|i| (uv[e.0][i] + uv[e.1][i]) * 0.5);
                (e, point(p))
            })
            .collect();
        let mut orientation = |p: [Uv; 3], vertices: [P3; 3]| -> Result<f64, String> {
            let u = (p[0][0] + p[1][0] + p[2][0]) / 3.;
            let v = (p[0][1] + p[1][1] + p[2][1]) / 3.;
            let h = u * (1. - u) * v * (1. - v);
            let s = u + v - 1.;
            let g = (v - 0.5).powi(2) * (v - 5. / 6.).powi(2);
            let gu =
                2. * (v - 0.5) * (v - 5. / 6.).powi(2) + 2. * (v - 0.5).powi(2) * (v - 5. / 6.);
            let du = 10000. * (((1. - 2. * u) * v * (1. - v) * s * s + 2. * h * s) * g);
            let dv =
                10000. * ((u * (1. - u) * (1. - 2. * v) * s * s + 2. * h * s) * g + h * s * s * gu);
            let a: P3 = std::array::from_fn(|i| vertices[1][i] - vertices[0][i]);
            let b: P3 = std::array::from_fn(|i| vertices[2][i] - vertices[0][i]);
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            Ok((-n[0] * du - n[1] * dv + n[2])
                / (n[0].hypot(n[1]).hypot(n[2]) * du.hypot(dv).hypot(1.)))
        };
        let parents =
            triangles.map(|t| orientation(t.map(|i| uv[i]), t.map(|i| points[i])).unwrap());
        assert!(parents.iter().all(|n| *n > 0.999999));
        let marked = BTreeSet::from([(0, 2), (2, 3)]);
        let obligations = marked
            .iter()
            .enumerate()
            .map(|(i, e)| crate::priority_refinement::Obligation {
                normal_failure: false,
                severity: 2.,
                identity: (1, i, 0),
                edges: vec![*e],
            })
            .collect();
        let selection = crate::priority_refinement::select(
            obligations,
            &marked,
            crate::priority_refinement::Budget {
                vertices: 4,
                triangles: 2,
                edges: 5,
                chord_remaining: 400000,
                surface_remaining: 1000000,
            },
        )
        .unwrap();
        let r = check(
            Mesh {
                uv: &uv,
                points: &points,
                triangles: &triangles,
                midpoint_points: &midpoints,
                parent_normals: &parents,
                target: 0.99,
            },
            selection,
            &mut orientation,
            &mut || Ok(1000000),
        )
        .unwrap();
        assert!(r.complete);
        assert!(r.selection.edges.is_empty());
        assert_eq!(
            r.selection.report["transitionQualityPreflight"]["passes"],
            3
        );
        assert_eq!(
            r.selection.report["transitionQualityPreflight"]["prunedGroupCount"],
            2
        );
    }
}
