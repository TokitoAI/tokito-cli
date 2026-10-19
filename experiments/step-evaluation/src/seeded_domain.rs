//! Conservative proposal filtering; final CDT topology must independently retain every seed.
use serde_json::{json, Value};
use std::collections::BTreeSet;
type Uv = [f64; 2];

pub(crate) fn axis(breaks: &[f64], range: Uv, period: Option<f64>) -> Result<Vec<f64>, String> {
    if breaks.len() < 2
        || breaks.len() > 257
        || breaks.iter().any(|p| !p.is_finite())
        || breaks.windows(2).any(|p| p[0] >= p[1])
        || range.iter().any(|p| !p.is_finite())
        || range[0] >= range[1]
    {
        return Err("surface_grid_invalid_axis_source".into());
    }
    let mut result = vec![range[0], range[1]];
    let (lo, hi) = (breaks[0], breaks[breaks.len() - 1]);
    if let Some(period) = period {
        if !period.is_finite() || period <= 0. || range[1] - range[0] > 4. * period {
            return Err("surface_grid_period_lift_budget".into());
        }
        let first = ((range[0] - hi) / period).floor();
        let last = ((range[1] - lo) / period).ceil();
        if !first.is_finite()
            || !last.is_finite()
            || first.abs() > 1_000_000.
            || last.abs() > 1_000_000.
            || last - first > 8.
        {
            return Err("surface_grid_period_lift_budget".into());
        }
        for shift in first as i64..=last as i64 {
            for &knot in breaks {
                let value = knot + shift as f64 * period;
                if range[0] < value && value < range[1] {
                    result.push(value);
                }
            }
        }
    } else {
        if range[0] < lo || range[1] > hi {
            return Err("surface_grid_axis_outside_source_domain".into());
        }
        result.extend(
            breaks
                .iter()
                .copied()
                .filter(|v| range[0] < *v && *v < range[1]),
        );
    }
    result.sort_by(f64::total_cmp);
    result.dedup();
    if result.len() > 257 {
        return Err("surface_grid_axis_break_budget".into());
    }
    Ok(result)
}
fn key(p: Uv) -> (u64, u64) {
    let bit = |v: f64| if v == 0. { 0 } else { v.to_bits() };
    (bit(p[0]), bit(p[1]))
}

pub(crate) fn append(boundary: &mut Vec<Uv>, proposals: Vec<Uv>) -> Result<Value, String> {
    let count = boundary.len();
    if !(3..=4096).contains(&count)
        || proposals.len() > 8192
        || boundary
            .iter()
            .chain(&proposals)
            .flatten()
            .any(|p| !p.is_finite())
    {
        return Err("surface_grid_invalid_domain_inventory".into());
    }
    let extent: [f64; 2] = std::array::from_fn(|a| {
        boundary
            .iter()
            .map(|p| p[a])
            .fold(f64::NEG_INFINITY, f64::max)
            - boundary.iter().map(|p| p[a]).fold(f64::INFINITY, f64::min)
    });
    let epsilon = 128. * f64::EPSILON * extent[0].max(extent[1]);
    if !epsilon.is_finite() || epsilon <= 0. {
        return Err("surface_grid_invalid_domain_extent".into());
    }
    let mut seen = boundary.iter().map(|p| key(*p)).collect::<BTreeSet<_>>();
    let mut rejected = [0usize; 3]; // duplicate, near constraint, exterior
    let mut accepted = Vec::new();
    let mut checks = 0usize;
    let mut new_points = Vec::new();
    for (index, p) in proposals.iter().copied().enumerate() {
        if seen.contains(&key(p)) {
            rejected[0] += 1;
            continue;
        }
        let mut near = false;
        let mut inside = false;
        for i in 0..count {
            checks += 1;
            if checks > 8_000_000 {
                return Err("surface_grid_domain_predicate_budget".into());
            }
            let a = boundary[i];
            let b = boundary[(i + 1) % count];
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = d[0].hypot(d[1]);
            if !length.is_finite() || length == 0. {
                return Err("surface_grid_invalid_domain_edge".into());
            }
            let signed = d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0]);
            if signed.abs() <= epsilon * length
                && (0..2)
                    .all(|j| p[j] >= a[j].min(b[j]) - epsilon && p[j] <= a[j].max(b[j]) + epsilon)
            {
                near = true;
                break;
            }
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if !x.is_finite() {
                    return Err("surface_grid_nonfinite_domain_predicate".into());
                }
                if p[0] < x {
                    inside = !inside;
                }
            }
        }
        if near {
            rejected[1] += 1;
        } else if !inside {
            rejected[2] += 1;
        } else {
            seen.insert(key(p));
            new_points.push(p);
            accepted.push(index);
        }
    }
    // Mutation happens only after the complete bounded filtering operation.
    boundary.extend(new_points);
    Ok(
        json!({"proposalCount":proposals.len(),"acceptedCount":accepted.len(),
        "acceptedProposalIndices":accepted,"duplicateProposalCount":rejected[0],
        "nearConstraintProposalCount":rejected[1],"exteriorProposalCount":rejected[2],
        "predicateCount":checks,"predicateLimit":8_000_000,"boundaryCoordinateTolerance":epsilon,
        "scope":"conservative_uv_proposal_filter_followed_by_independent_constrained_domain_topology_checks",
        "originalBoundaryVertexCount":count,"geometryQualified":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concave_domain_retains_only_interior_and_does_not_split_constraints() {
        let original = vec![[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]];
        let mut b = original.clone();
        let r = append(
            &mut b,
            vec![[0.5, 0.5], [1.5, 1.5], [1., 1.5], [0., 0.], [0.5, 0.5]],
        )
        .unwrap();
        assert_eq!(&b[..6], original);
        assert_eq!(&b[6..], [[0.5, 0.5]]);
        assert_eq!(r["acceptedProposalIndices"], json!([0]));
        assert_eq!(r["exteriorProposalCount"], 1);
        assert_eq!(r["nearConstraintProposalCount"], 1);
        assert_eq!(r["duplicateProposalCount"], 2);
    }
    #[test]
    fn clipping_preserves_knots_and_periodic_lifts_and_refuses_bad_ranges() {
        assert_eq!(
            axis(&[0., 0.2, 1.], [0.1, 0.8], None).unwrap(),
            vec![0.1, 0.2, 0.8]
        );
        let x = axis(&[0., 0.25, 1.], [-0.9, 0.4], Some(1.)).unwrap();
        assert_eq!(x, vec![-0.9, -0.75, 0., 0.25, 0.4]);
        assert!(axis(&[0., 1.], [-0.1, 1.], None).is_err());
        assert!(axis(&[0., 1.], [0., 5.], Some(1.)).is_err());
        assert!(axis(&[0., 0., 1.], [0., 1.], None).is_err());
    }
    #[test]
    fn failed_filter_never_partially_mutates_boundary() {
        let mut b = vec![[0., 0.], [1., 0.], [1., 1.], [0., 1.]];
        let original = b.clone();
        assert!(append(&mut b, vec![[0.5, 0.5], [f64::NAN, 0.]]).is_err());
        assert_eq!(b, original);
    }
}
