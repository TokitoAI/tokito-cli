//! Bounded physical chord probes for knot-preserving tensor seed proposals.
//! Passing these discrete probes is not a surface-accuracy or solid qualification.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

const MAX_POINTS: usize = 8192;
const MAX_AXIS: usize = 257;
const MAX_ROUNDS: usize = 12;
const MAX_EVALUATIONS: usize = 60_000;
const MAX_CELLS: usize = 8192;

pub(crate) struct Grid {
    pub points: Vec<[f64; 2]>,
    pub report: Value,
}

fn key(p: [f64; 2]) -> (u64, u64) {
    let bits = |x: f64| if x == 0.0 { 0 } else { x.to_bits() };
    (bits(p[0]), bits(p[1]))
}
fn midpoint(a: f64, b: f64) -> Result<f64, String> {
    let m = a * 0.5 + b * 0.5;
    if m.is_finite() && a < m && m < b {
        Ok(m)
    } else {
        Err("surface grid interval has no representable finite midpoint".into())
    }
}
fn validate(axis: &[f64]) -> Result<(), String> {
    if axis.len() < 2
        || axis.iter().any(|x| !x.is_finite())
        || axis.windows(2).any(|w| w[0] >= w[1])
    {
        return Err("surface grid requires strictly increasing finite axis breaks".into());
    }
    Ok(())
}
fn probes(u: &[f64], v: &[f64]) -> Result<BTreeMap<(u64, u64), [f64; 2]>, String> {
    let mut result = BTreeMap::new();
    for a in u.windows(2) {
        let us = [a[0], midpoint(a[0], a[1])?, a[1]];
        for b in v.windows(2) {
            let vs = [b[0], midpoint(b[0], b[1])?, b[1]];
            for x in us {
                for y in vs {
                    result.insert(key([x, y]), [x, y]);
                }
            }
        }
    }
    Ok(result)
}
fn chord(a: [f64; 3], b: [f64; 3], m: [f64; 3]) -> Result<f64, String> {
    let d = std::array::from_fn::<_, 3, _>(|i| m[i] - (a[i] * 0.5 + b[i] * 0.5));
    let error = d[0].hypot(d[1]).hypot(d[2]);
    if error.is_finite() {
        Ok(error)
    } else {
        Err("nonfinite surface grid chord error".into())
    }
}
fn refined(axis: &[f64], marked: &BTreeSet<usize>) -> Result<Vec<f64>, String> {
    let mut out = Vec::with_capacity(axis.len() + marked.len());
    for (i, w) in axis.windows(2).enumerate() {
        out.push(w[0]);
        if marked.contains(&i) {
            out.push(midpoint(w[0], w[1])?);
        }
    }
    out.push(axis[axis.len() - 1]);
    Ok(out)
}
fn limit(u: &[f64], v: &[f64]) -> Option<&'static str> {
    if u.len() > MAX_AXIS || v.len() > MAX_AXIS {
        Some("axis_limit")
    } else if (u.len() - 2).saturating_mul(v.len() - 2) > MAX_POINTS {
        Some("interior_point_limit")
    } else if (u.len() - 1).saturating_mul(v.len() - 1) > MAX_CELLS {
        Some("cell_limit")
    } else {
        None
    }
}

pub(crate) fn sample<F>(
    u_breaks: &[f64],
    v_breaks: &[f64],
    tolerance: f64,
    mut evaluate: F,
) -> Result<Grid, String>
where
    F: FnMut([f64; 2]) -> Result<[f64; 3], String>,
{
    if u_breaks.len() > MAX_AXIS || v_breaks.len() > MAX_AXIS {
        return Err("surface grid input axis exceeds 257-break limit".into());
    }
    validate(u_breaks)?;
    validate(v_breaks)?;
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err("surface grid tolerance must be positive and finite".into());
    }
    let mut u = u_breaks.to_vec();
    let mut v = v_breaks.to_vec();
    let mut cache: BTreeMap<(u64, u64), [f64; 3]> = BTreeMap::new();
    let mut trace = Vec::new();
    let mut tested = false;
    let mut target_met = false;
    let mut truncation = None;
    let mut probe_count = 0usize;
    let mut final_errors = Value::Null;
    // Only commit a proposed grid after reserving its complete measurement pass.
    for round in 0..=MAX_ROUNDS {
        if let Some(reason) = limit(&u, &v) {
            truncation = Some(reason);
            break;
        }
        let required = probes(&u, &v)?;
        let new_count = required.keys().filter(|k| !cache.contains_key(*k)).count();
        if cache.len() + new_count > MAX_EVALUATIONS {
            truncation = Some("evaluation_limit_before_initial_grid");
            break;
        }
        for (k, uv) in required {
            if let std::collections::btree_map::Entry::Vacant(entry) = cache.entry(k) {
                let position = evaluate(uv)?;
                if position.iter().any(|x| !x.is_finite()) {
                    return Err("surface grid evaluator returned nonfinite coordinates".into());
                }
                entry.insert(position);
            }
        }
        let mut marked_u = BTreeSet::new();
        let mut marked_v = BTreeSet::new();
        let mut maxima = [0.0_f64; 3];
        let mut worst_uv = [[0.0; 2]; 3];
        for (i, a) in u.windows(2).enumerate() {
            let us = [a[0], midpoint(a[0], a[1])?, a[1]];
            for (j, b) in v.windows(2).enumerate() {
                let vs = [b[0], midpoint(b[0], b[1])?, b[1]];
                let position = |x: usize, y: usize| cache[&key([us[x], vs[y]])];
                let mut record = |kind: usize, error: f64, uv: [f64; 2]| {
                    probe_count += 1;
                    if error > maxima[kind] {
                        maxima[kind] = error;
                        worst_uv[kind] = uv;
                    }
                };
                let mut u_failure = false;
                let mut v_failure = false;
                for k in 0..3 {
                    let e = chord(position(0, k), position(2, k), position(1, k))?;
                    record(0, e, [us[1], vs[k]]);
                    if e > tolerance {
                        marked_u.insert(i);
                        u_failure = true;
                    }
                    let e = chord(position(k, 0), position(k, 2), position(k, 1))?;
                    record(1, e, [us[k], vs[1]]);
                    if e > tolerance {
                        marked_v.insert(j);
                        v_failure = true;
                    }
                }
                // Both triangle diagonals: bilinear interpolation alone misses saddles.
                for ((ax, ay), (bx, by)) in [((0, 0), (2, 2)), ((0, 2), (2, 0))] {
                    let e = chord(position(ax, ay), position(bx, by), position(1, 1))?;
                    record(2, e, [us[1], vs[1]]);
                    if e > tolerance {
                        // Attribute known directional bending before splitting both
                        // axes. A pure mixed-term saddle fails neither axial probe.
                        if u_failure || !v_failure {
                            marked_u.insert(i);
                        }
                        if v_failure || !u_failure {
                            marked_v.insert(j);
                        }
                    }
                }
            }
        }
        tested = true;
        final_errors = json!({"u_chord":maxima[0],"v_chord":maxima[1],
            "diagonal_chord":maxima[2],"worst_uv":worst_uv});
        target_met = marked_u.is_empty() && marked_v.is_empty();
        trace.push(json!({"round":round,"u_count":u.len(),"v_count":v.len(),
            "cells":(u.len()-1)*(v.len()-1),"new_evaluations":new_count,
            "unique_evaluations":cache.len(),"marked_u_intervals":marked_u,
            "marked_v_intervals":marked_v,"sampled_target_met":target_met,
            "errors":final_errors}));
        if target_met {
            break;
        }
        if round == MAX_ROUNDS {
            truncation = Some("round_limit");
            break;
        }
        // Even a one-directional bend needs interior cross-sections for seed delivery.
        if !marked_u.is_empty() && v.len() == 2 {
            marked_v.insert(0);
        }
        if !marked_v.is_empty() && u.len() == 2 {
            marked_u.insert(0);
        }
        let next_u = refined(&u, &marked_u)?;
        let next_v = refined(&v, &marked_v)?;
        if let Some(reason) = limit(&next_u, &next_v) {
            truncation = Some(reason);
            break;
        }
        let next_probes = probes(&next_u, &next_v)?;
        let needed = next_probes
            .keys()
            .filter(|k| !cache.contains_key(*k))
            .count();
        if cache.len() + needed > MAX_EVALUATIONS {
            truncation = Some("evaluation_limit");
            break;
        }
        u = next_u;
        v = next_v;
    }
    let mut points = Vec::new();
    if tested {
        for &x in &u[1..u.len() - 1] {
            for &y in &v[1..v.len() - 1] {
                points.push([x, y]);
            }
        }
    }
    Ok(Grid {
        report: json!({"algorithm":"bounded_knot_directional_chord_grid_v1",
            "status":if target_met {"sampled_target_met"} else {"incomplete"},
            "qualified":false,"accuracy_proven":false,"trim_filtered":false,
            "grid_tested":tested,"sampled_target_met":target_met,"truncation":truncation,
            "tolerance":tolerance,"u_axis":u,"v_axis":v,
            "preserved_u_breaks":u_breaks,"preserved_v_breaks":v_breaks,
            "interior_proposals":points.len(),"unique_evaluations":cache.len(),
            "chord_probes":probe_count,"final_errors":final_errors,"rounds":trace,
            "limits":{"interior_points":MAX_POINTS,"axis_points":MAX_AXIS,
                "subdivision_rounds":MAX_ROUNDS,"unique_evaluations":MAX_EVALUATIONS,
                "cells_per_round":MAX_CELLS},
            "scope":"discrete midpoint probes only; caller must filter interior proposals against trim"}),
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn axis(g: &Grid, name: &str) -> Vec<f64> {
        g.report[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect()
    }
    #[test]
    fn affine_plane_has_no_unnecessary_seeds() {
        let mut calls = 0;
        let g = sample(&[0., 1.], &[0., 1.], 1e-8, |[u, v]| {
            calls += 1;
            Ok([u, v, 2. * u + 3. * v])
        })
        .unwrap();
        assert!(g.points.is_empty());
        assert_eq!(calls, 9);
        assert_eq!(g.report["unique_evaluations"], calls);
        assert_eq!(g.report["sampled_target_met"], true);
        assert_eq!(g.report["qualified"], false);
    }
    #[test]
    fn quadratic_refines_curved_direction_with_cross_section() {
        let g = sample(&[0., 1.], &[0., 1.], 0.01, |[u, v]| Ok([u, v, u * u])).unwrap();
        assert!(axis(&g, "u_axis").len() > axis(&g, "v_axis").len());
        assert!(axis(&g, "v_axis").len() >= 3);
        assert!(g
            .points
            .iter()
            .all(|p| p[0] > 0. && p[0] < 1. && p[1] > 0. && p[1] < 1.));
        assert_eq!(g.report["sampled_target_met"], true);
    }
    #[test]
    fn both_diagonals_detect_bilinear_saddle() {
        let g = sample(&[0., 1.], &[0., 1.], 0.02, |[u, v]| Ok([u, v, u * v])).unwrap();
        assert!(axis(&g, "u_axis").len() > 2 && axis(&g, "v_axis").len() > 2);
        assert_eq!(g.report["rounds"][0]["errors"]["u_chord"], 0.);
        assert_eq!(g.report["rounds"][0]["errors"]["v_chord"], 0.);
        assert_eq!(g.report["rounds"][0]["errors"]["diagonal_chord"], 0.25);
        assert_eq!(g.report["sampled_target_met"], true);
    }
    #[test]
    fn anisotropic_parameter_units_do_not_change_physical_sampling() {
        let a = sample(&[0., 1.], &[0., 1.], 0.01, |[u, v]| Ok([u, v, u * u])).unwrap();
        let b = sample(&[0., 1000.], &[0., 0.001], 0.01, |[u, v]| {
            let x = u / 1000.;
            Ok([x, v / 0.001, x * x])
        })
        .unwrap();
        assert_eq!(a.points.len(), b.points.len());
        assert_eq!(axis(&a, "u_axis").len(), axis(&b, "u_axis").len());
        assert_eq!(axis(&a, "v_axis").len(), axis(&b, "v_axis").len());
    }
    #[test]
    fn internal_knots_are_retained_exactly() {
        let knots = [0., 0.3, 0.7, 1.];
        let g = sample(&knots, &[0., 0.2, 1.], 0.005, |[u, v]| Ok([u, v, u * u])).unwrap();
        for k in knots {
            assert!(axis(&g, "u_axis").contains(&k));
        }
        assert!(axis(&g, "v_axis").contains(&0.2));
        assert_eq!(g.report["preserved_u_breaks"], json!(knots));
    }
    #[test]
    fn truncation_keeps_last_fully_measured_grid() {
        let g = sample(&[0., 1.], &[0., 1.], 1e-30, |[u, v]| {
            Ok([u, v, u * u + v * v])
        })
        .unwrap();
        assert_eq!(g.report["grid_tested"], true);
        assert_eq!(g.report["sampled_target_met"], false);
        assert_eq!(g.report["status"], "incomplete");
        assert!(g.report["truncation"].is_string());
        assert!(g.points.len() <= MAX_POINTS);
        assert!(g.report["unique_evaluations"].as_u64().unwrap() <= MAX_EVALUATIONS as u64);
        let last = g.report["rounds"].as_array().unwrap().last().unwrap();
        assert_eq!(
            last["u_count"].as_u64().unwrap() as usize,
            axis(&g, "u_axis").len()
        );
        assert_eq!(
            last["v_count"].as_u64().unwrap() as usize,
            axis(&g, "v_axis").len()
        );
        assert_eq!(last["errors"], g.report["final_errors"]);
    }
    #[test]
    fn oversized_initial_grid_is_not_claimed_tested() {
        let knots: Vec<_> = (0..=256).map(|x| x as f64).collect();
        let g = sample(&knots, &knots, 0.01, |_| panic!("must not evaluate")).unwrap();
        assert_eq!(g.report["grid_tested"], false);
        assert!(g.points.is_empty());
        assert_eq!(g.report["unique_evaluations"], 0);
    }
    #[test]
    fn invalid_inputs_and_evaluator_errors_are_rejected() {
        let oversized: Vec<_> = (0..=257).map(|x| x as f64).collect();
        assert!(sample(&oversized, &[0., 1.], 0.1, |_| panic!("must not evaluate")).is_err());
        for bad in [
            vec![],
            vec![0.],
            vec![0., 0.],
            vec![1., 0.],
            vec![0., f64::NAN],
        ] {
            assert!(sample(&bad, &[0., 1.], 0.1, |_| Ok([0.; 3])).is_err());
        }
        assert!(sample(&[0., 1.], &[0., 1.], 0., |_| Ok([0.; 3])).is_err());
        assert!(
            matches!(sample(&[0.,1.],&[0.,1.],0.1,|_|Err("oracle failed".into())),Err(e) if e.contains("oracle failed"))
        );
        assert!(sample(&[0., 1.], &[0., 1.], 0.1, |_| Ok([f64::INFINITY, 0., 0.])).is_err());
    }
}
