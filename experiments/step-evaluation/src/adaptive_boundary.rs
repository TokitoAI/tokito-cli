//! Bounded source-parameter sampling. Sampled chord tests are not a CAD certificate.
use serde_json::{json, Value};

pub(crate) type Uv = [f64; 2];
type P3 = [f64; 3];
const MAX_POINTS: usize = 4096;
const MAX_EVALUATIONS: usize = 16384;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug)]
pub(crate) struct Sample {
    pub edge: usize,
    pub parameter: f64,
    pub uv: Uv,
    pub source: P3,
    pub surface: P3,
    pub projection_converged: bool,
    pub projection_solver: Value,
    pub original_index: Option<usize>,
}

impl Sample {
    fn valid(&self) -> bool {
        self.uv
            .iter()
            .chain(&self.source)
            .chain(&self.surface)
            .chain(std::iter::once(&self.parameter))
            .all(|x| x.is_finite())
            && distance(self.source, self.surface).is_finite()
            && self.projection_solver.is_object()
    }
    pub(crate) fn report(&self) -> Value {
        json!({"loopIndex":self.edge,"curveParameter":self.parameter,"candidateUv":self.uv,
            "sourcePointSourceUnits":self.source,"surfacePointSourceUnits":self.surface,
            "surfaceResidualSourceUnits":distance(self.source,self.surface),
            "projectionConverged":self.projection_converged,"projectionSolver":self.projection_solver,"originalInputIndex":self.original_index})
    }
}

fn distance(a: P3, b: P3) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}
fn lerp<const N: usize>(a: [f64; N], b: [f64; N], f: f64) -> [f64; N] {
    std::array::from_fn(|i| a[i] * (1. - f) + b[i] * f)
}

#[derive(Debug)]
pub(crate) struct ResultSamples {
    pub samples: Vec<Sample>,
    pub report: Value,
}

pub(crate) fn subdivide<C, F>(
    input: &[Sample],
    tolerance: f64,
    mut curve_project: C,
    mut surface: F,
) -> Result<ResultSamples, String>
where
    C: FnMut(usize, f64, Uv) -> Result<Sample, String>,
    F: FnMut(Uv) -> Result<P3, String>,
{
    subdivide_bounded(
        input,
        tolerance,
        MAX_POINTS,
        MAX_EVALUATIONS,
        &mut curve_project,
        &mut surface,
    )
}

fn subdivide_bounded<C, F>(
    input: &[Sample],
    tolerance: f64,
    point_limit: usize,
    evaluation_limit: usize,
    curve_project: &mut C,
    surface: &mut F,
) -> Result<ResultSamples, String>
where
    C: FnMut(usize, f64, Uv) -> Result<Sample, String>,
    F: FnMut(Uv) -> Result<P3, String>,
{
    if !tolerance.is_finite()
        || tolerance <= 0.
        || input.len() < 4
        || input.len() > point_limit
        || point_limit > MAX_POINTS
        || evaluation_limit > MAX_EVALUATIONS
        || input
            .iter()
            .enumerate()
            .any(|(i, p)| !p.valid() || p.original_index != Some(i))
    {
        return Err("adaptive_boundary_invalid_input_or_limits".into());
    }
    // Edge-local runs include both endpoints. Preserve all junction duplicates.
    let mut direction = None;
    for pair in input.windows(2) {
        let [a, b] = [&pair[0], &pair[1]];
        if a.edge == b.edge {
            let d = b.parameter - a.parameter;
            if !d.is_finite() || d == 0. || direction.is_some_and(|v| v != (d > 0.)) {
                return Err("adaptive_boundary_parameter_order_invalid".into());
            }
            direction = Some(d > 0.);
        } else {
            if b.edge != a.edge + 1 || a.uv != b.uv || a.source != b.source {
                return Err("adaptive_boundary_source_junction_mismatch".into());
            }
            direction = None;
        }
    }
    if input[0].edge != 0
        || input[0].uv != input.last().unwrap().uv
        || input[0].source != input.last().unwrap().source
    {
        return Err("adaptive_boundary_source_loop_not_closed".into());
    }
    let mut output = vec![input[0].clone()];
    let mut intervals = Vec::new();
    let mut observations = Vec::new();
    let mut evaluations = 0;
    let mut splits = 0;
    let mut maximum_depth = 0;
    for pair in input.windows(2) {
        if pair[0].edge != pair[1].edge {
            output.push(pair[1].clone());
            continue;
        }
        let mut stack = vec![(pair[0].clone(), pair[1].clone(), 0usize)];
        while let Some((a, b, depth)) = stack.pop() {
            maximum_depth = maximum_depth.max(depth);
            let mut curve_error: f64 = 0.;
            let mut support_error: f64 = 0.;
            let mut midpoint = None;
            let mut probes = Vec::new();
            for fraction in [0.25, 0.5, 0.75] {
                evaluations += 1;
                if evaluations > evaluation_limit {
                    return Err("adaptive_boundary_sample_budget_exceeded".into());
                }
                let parameter = a.parameter * (1. - fraction) + b.parameter * fraction;
                if !parameter.is_finite()
                    || parameter <= a.parameter.min(b.parameter)
                    || parameter >= a.parameter.max(b.parameter)
                {
                    return Err("adaptive_boundary_parameter_midpoint_stagnated".into());
                }
                let seed = lerp(a.uv, b.uv, fraction);
                let point = curve_project(a.edge, parameter, seed)?;
                if !point.valid()
                    || point.edge != a.edge
                    || point.parameter != parameter
                    || point.original_index.is_some()
                {
                    return Err("adaptive_boundary_evaluator_identity_or_finite_failure".into());
                }
                if point.uv == a.uv || point.uv == b.uv {
                    return Err("adaptive_boundary_projected_uv_stagnated".into());
                }
                let support_point = surface(seed)?;
                if support_point.iter().any(|x| !x.is_finite()) {
                    return Err("adaptive_boundary_surface_nonfinite".into());
                }
                curve_error =
                    curve_error.max(distance(point.source, lerp(a.source, b.source, fraction)));
                support_error = support_error.max(distance(
                    support_point,
                    lerp(a.surface, b.surface, fraction),
                ));
                probes.push(json!({"fraction":fraction,"sample":point.report(),
                    "linearUv":seed,"supportAtLinearUvSourceUnits":support_point}));
                if fraction == 0.5 {
                    midpoint = Some(point);
                }
            }
            if !curve_error.is_finite() || !support_error.is_finite() {
                return Err("adaptive_boundary_nonfinite_chord_error".into());
            }
            observations.push(
                json!({"start":a.report(),"end":b.report(),"depth":depth,"probes":probes,
                "maximumSampledSourceCurveChordErrorSourceUnits":curve_error,
                "maximumSampledSupportChordErrorSourceUnits":support_error,
                "split":curve_error>tolerance || support_error>tolerance}),
            );
            if curve_error > tolerance || support_error > tolerance {
                if depth >= MAX_DEPTH {
                    return Err("adaptive_boundary_depth_budget_exceeded".into());
                }
                splits += 1;
                if input.len() + splits > point_limit {
                    return Err("adaptive_boundary_vertex_budget_exceeded".into());
                }
                let middle = midpoint.unwrap();
                stack.push((middle.clone(), b, depth + 1));
                stack.push((a, middle, depth + 1));
            } else {
                let start = output.len() - 1;
                output.push(b);
                intervals.push(json!({"startInputIndex":start,"endInputIndex":start+1,
                    "loopIndex":a.edge,"depth":depth,"maximumSampledSourceCurveChordErrorSourceUnits":curve_error,
                    "maximumSampledSupportChordErrorSourceUnits":support_error}));
            }
        }
    }
    if output.len() != input.len() + splits {
        return Err("adaptive_boundary_inventory_mismatch".into());
    }
    let original_mapping = output
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.original_index.map(|j| (j, i)))
        .collect::<Vec<_>>();
    if original_mapping.len() != input.len()
        || original_mapping.iter().enumerate().any(|(j, (k, i))| {
            *k != j || output[*i].uv != input[j].uv || output[*i].source != input[j].source
        })
    {
        return Err("adaptive_boundary_original_sample_changed".into());
    }
    let report = json!({"schema":"tokito.adaptive-source-boundary.v1","status":"sampled_chord_targets_met_unqualified",
        "toleranceSourceUnits":tolerance,"originalInputSampleCount":input.len(),"expandedInputSampleCount":output.len(),
        "addedSampleCount":splits,"originalSampleExpandedInputIndices":original_mapping.iter().map(|(_,i)|i).collect::<Vec<_>>(),
        "samples":output.iter().map(Sample::report).collect::<Vec<_>>(),"intervals":intervals,"observations":observations,
        "sampleFractions":[0.25,0.5,0.75],"curveProjectionEvaluationCount":evaluations,"curveProjectionEvaluationLimit":evaluation_limit,
        "retainedPointLimit":point_limit,"depthLimit":MAX_DEPTH,"maximumDepth":maximum_depth,
        "sourceCoordinatesAndOriginalUvPreserved":true,"sharedAcrossFaces":false,
        "projectionAdmissionToleranceChanged":false,"sourceCurveToSupportSurfaceGapResolved":false,
        "certifiedCurveDeviationBound":false,"geometryQualified":false,"admitted":false,"exportsProduced":[]});
    Ok(ResultSamples {
        samples: output,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(edge: usize, t: f64, index: Option<usize>) -> Sample {
        let uv = match edge {
            0 => [t, 0.],
            1 => [1., t],
            2 => [1. - t, 1.],
            _ => [0., 1. - t],
        };
        let source = [uv[0], uv[1], uv[0] * (1. - uv[0])];
        Sample {
            edge,
            parameter: t,
            uv,
            source,
            surface: source,
            projection_converged: true,
            projection_solver: json!({"scope":"analytic_test"}),
            original_index: index,
        }
    }
    fn seeds() -> Vec<Sample> {
        (0..4)
            .flat_map(|e| (0..2).map(move |i| sample(e, i as f64, Some(e * 2 + i))))
            .collect()
    }
    #[test]
    fn curved_boundary_preserves_junctions_source_identities_and_meets_sampled_targets() {
        let input = seeds();
        let r = subdivide(
            &input,
            0.001,
            |e, t, _| Ok(sample(e, t, None)),
            |uv| Ok([uv[0], uv[1], uv[0] * (1. - uv[0])]),
        )
        .unwrap();
        assert!(r.samples.len() > input.len());
        assert_eq!(r.report["geometryQualified"], false);
        for interval in r.report["intervals"].as_array().unwrap() {
            assert!(
                interval["maximumSampledSourceCurveChordErrorSourceUnits"]
                    .as_f64()
                    .unwrap()
                    <= 0.001
            );
            assert!(
                interval["maximumSampledSupportChordErrorSourceUnits"]
                    .as_f64()
                    .unwrap()
                    <= 0.001
            );
        }
        for (i, k) in r.report["originalSampleExpandedInputIndices"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let p = &r.samples[k.as_u64().unwrap() as usize];
            assert_eq!(p.uv, input[i].uv);
            assert_eq!(p.source, input[i].source);
        }
    }
    #[test]
    fn budgets_evaluator_failure_invalid_identity_and_broken_junction_refuse() {
        let input = seeds();
        let evaluate = |uv: Uv| Ok([uv[0], uv[1], uv[0] * (1. - uv[0])]);
        assert!(subdivide_bounded(
            &input,
            0.001,
            input.len(),
            MAX_EVALUATIONS,
            &mut |e, t, _| Ok(sample(e, t, None)),
            &mut evaluate.clone()
        )
        .unwrap_err()
        .contains("vertex_budget"));
        assert!(subdivide_bounded(
            &input,
            0.001,
            MAX_POINTS,
            1,
            &mut |e, t, _| Ok(sample(e, t, None)),
            &mut evaluate.clone()
        )
        .unwrap_err()
        .contains("sample_budget"));
        assert!(subdivide(&input, 0.001, |_, _, _| Err("failure".into()), evaluate).is_err());
        assert!(subdivide(&input, 0.001, |e, t, _| Ok(sample(e, t, Some(0))), evaluate).is_err());
        let mut bad = input.clone();
        bad[2].uv[0] = 0.9;
        assert!(subdivide(&bad, 0.001, |e, t, _| Ok(sample(e, t, None)), evaluate).is_err());
        assert!(subdivide(&input, f64::NAN, |e, t, _| Ok(sample(e, t, None)), evaluate).is_err());
    }
    #[test]
    fn straight_edges_do_not_split_and_decreasing_parameters_are_supported() {
        let mut input = seeds();
        for p in &mut input {
            p.source[2] = 0.;
            p.surface[2] = 0.;
        }
        let r = subdivide(
            &input,
            1e-9,
            |e, t, _| {
                let mut p = sample(e, t, None);
                p.source[2] = 0.;
                p.surface[2] = 0.;
                Ok(p)
            },
            |uv| Ok([uv[0], uv[1], 0.]),
        )
        .unwrap();
        assert_eq!(r.report["addedSampleCount"], 0);
        let mut reverse = input.iter().rev().cloned().collect::<Vec<_>>();
        for (i, p) in reverse.iter_mut().enumerate() {
            p.edge = 3 - p.edge;
            p.original_index = Some(i);
        }
        let r = subdivide(
            &reverse,
            1e-9,
            |e, t, _| {
                let mut p = sample(3 - e, t, None);
                p.edge = e;
                p.source[2] = 0.;
                p.surface[2] = 0.;
                Ok(p)
            },
            |uv| Ok([uv[0], uv[1], 0.]),
        )
        .unwrap();
        assert_eq!(r.report["addedSampleCount"], 0);
    }
}
