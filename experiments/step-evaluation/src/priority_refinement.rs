//! Deterministic atomic split obligations and a complete-next-pass reservation.
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub(crate) type Edge = (usize, usize);
#[derive(Debug, Clone)]
pub(crate) struct Obligation {
    pub normal_failure: bool,
    pub severity: f64,
    // Stable triangle index, or the two endpoint indices of an edge obligation.
    pub identity: (usize, usize, usize),
    pub edges: Vec<Edge>,
}
#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub vertices: usize,
    pub triangles: usize,
    pub edges: usize,
    pub chord_remaining: usize,
    pub surface_remaining: usize,
}
pub(crate) struct Selection {
    pub edges: BTreeSet<Edge>,
    pub report: Value,
    pub groups: Vec<Obligation>,
    pub obligations: Vec<Obligation>,
}

pub(crate) fn select(
    mut obligations: Vec<Obligation>,
    marked: &BTreeSet<Edge>,
    b: Budget,
) -> Result<Selection, String> {
    if b.vertices > 25_000
        || b.triangles > 50_000
        || b.edges > 75_000
        || obligations.len() > 200_000
        || obligations.iter().any(|o| {
            !o.severity.is_finite()
                || o.severity < 0.
                || o.edges.len() > 3
                || o.edges.iter().any(|e| e.0 >= e.1 || !marked.contains(e))
        })
    {
        return Err("priority_refinement_invalid_obligations_or_budget".into());
    }
    let base_chord = b
        .triangles
        .checked_add(b.edges)
        .ok_or("priority_chord_cost_overflow")?;
    let base_surface = b
        .triangles
        .checked_mul(5)
        .and_then(|v| v.checked_add(b.edges))
        .ok_or("priority_surface_cost_overflow")?;
    let vertex_limit = 25_000 - b.vertices;
    let triangle_limit = (50_000 - b.triangles) / 2;
    let chord_limit = b
        .chord_remaining
        .checked_sub(base_chord)
        .map_or(0, |v| v / 5);
    let surface_limit = b
        .surface_remaining
        .checked_sub(base_surface)
        .map_or(0, |v| v / 13);
    let limit = vertex_limit
        .min(triangle_limit)
        .min(chord_limit)
        .min(surface_limit);
    obligations.sort_by(|a, b| {
        b.normal_failure
            .cmp(&a.normal_failure)
            .then_with(|| b.severity.total_cmp(&a.severity))
            .then_with(|| a.identity.cmp(&b.identity))
    });
    let mut selected = BTreeSet::new();
    let mut accepted = 0;
    let mut satisfied = 0;
    let mut deferred = 0;
    let mut normal_accepted = 0;
    let mut normal_deferred = 0;
    let mut groups = Vec::new();
    for o in &obligations {
        let added = o
            .edges
            .iter()
            .filter(|e| !selected.contains(*e))
            .copied()
            .collect::<BTreeSet<_>>();
        if added.is_empty() {
            if o.edges.is_empty() {
                deferred += 1;
                normal_deferred += usize::from(o.normal_failure);
            } else {
                satisfied += 1;
            }
        } else if selected.len() + added.len() <= limit {
            selected.extend(added);
            groups.push(o.clone());
            accepted += 1;
            normal_accepted += usize::from(o.normal_failure);
        } else {
            deferred += 1;
            normal_deferred += usize::from(o.normal_failure);
        }
    }
    let n = selected.len();
    let report = json!({"schema":"tokito.priority-interior-split-selection.v1",
        "ranking":"normal_failure_first_then_descending_sampled_chord_ratio_then_stable_triangle_or_edge_identity",
        "atomicTriangleObligations":true,"markedEdgeCount":marked.len(),"selectedEdgeCount":n,
        "deferredEdgeCount":marked.len()-n,"selectedEdges":selected.iter().map(|(a,b)|[*a,*b]).collect::<Vec<_>>(),
        "obligationCount":obligations.len(),"acceptedObligationCount":accepted,"alreadySatisfiedObligationCount":satisfied,
        "deferredObligationCount":deferred,"acceptedNormalObligationCount":normal_accepted,"deferredNormalObligationCount":normal_deferred,
        "budgetBefore":{"vertices":b.vertices,"triangles":b.triangles,"edges":b.edges,
            "remainingChordSamples":b.chord_remaining,"remainingSurfaceEvaluations":b.surface_remaining},
        "splitCapacity":{"vertex":vertex_limit,"triangle":triangle_limit,"chordSamples":chord_limit,"surfaceEvaluations":surface_limit,"minimum":limit},
        "reservedNextChordSamples":base_chord+5*n,"reservedNextSurfaceEvaluations":base_surface+13*n,
        "expectedNextVertices":b.vertices+n,"expectedNextTriangles":b.triangles+2*n,"expectedNextEdges":b.edges+3*n,
        "verticesMoved":0,"boundaryEdgesSplit":0,"geometryQualified":false,"admitted":false});
    Ok(Selection {
        edges: selected,
        report,
        groups,
        obligations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn o(normal: bool, severity: f64, id: usize, edges: &[Edge]) -> Obligation {
        Obligation {
            normal_failure: normal,
            severity,
            identity: (0, id, 0),
            edges: edges.to_vec(),
        }
    }
    fn budget() -> Budget {
        Budget {
            vertices: 24997,
            triangles: 49994,
            edges: 74990,
            chord_remaining: 400000,
            surface_remaining: 1000000,
        }
    }
    #[test]
    fn partial_atomic_normal_first_selection_and_shared_edge_union_are_deterministic() {
        let a = (0, 1);
        let b = (0, 2);
        let c = (1, 2);
        let d = (1, 3);
        let marked = BTreeSet::from([a, b, c, d]);
        let r = select(
            vec![
                o(false, 100., 0, &[d]),
                o(true, 2., 1, &[a, b]),
                o(true, 1., 2, &[b, c]),
            ],
            &marked,
            budget(),
        )
        .unwrap();
        assert_eq!(r.edges, BTreeSet::from([a, b, c]));
        assert_eq!(r.report["expectedNextVertices"], 25000);
        assert_eq!(r.report["expectedNextTriangles"], 50000);
        assert_eq!(r.report["acceptedNormalObligationCount"], 2);
        assert_eq!(r.report["deferredEdgeCount"], 1);
        let r = select(
            vec![o(true, 1., 0, &[a, b, c, d][..3])],
            &marked,
            Budget {
                vertices: 24998,
                ..budget()
            },
        )
        .unwrap();
        assert!(r.edges.is_empty()); // Triangle obligation cannot be partially honored.
    }
    #[test]
    fn complete_next_pass_chord_and_surface_reservations_limit_mutation() {
        let marked = BTreeSet::from([(0, 1)]);
        let b = Budget {
            vertices: 4,
            triangles: 2,
            edges: 5,
            chord_remaining: 12,
            surface_remaining: 28,
        };
        let r = select(vec![o(true, 1., 0, &[(0, 1)])], &marked, b).unwrap();
        assert_eq!(r.edges.len(), 1);
        assert_eq!(r.report["reservedNextChordSamples"], 12);
        assert_eq!(r.report["reservedNextSurfaceEvaluations"], 28);
        for b in [
            Budget {
                chord_remaining: 11,
                ..b
            },
            Budget {
                surface_remaining: 27,
                ..b
            },
        ] {
            assert!(select(vec![o(true, 1., 0, &[(0, 1)])], &marked, b)
                .unwrap()
                .edges
                .is_empty());
        }
        assert!(select(vec![o(false, f64::NAN, 0, &[(0, 1)])], &marked, b).is_err());
        assert!(select(vec![o(false, 1., 0, &[(0, 2)])], &marked, b).is_err());
    }
}
