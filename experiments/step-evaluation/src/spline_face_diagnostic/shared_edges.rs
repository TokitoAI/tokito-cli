//! Shared, source-derived curve probes on incident supports. No sewing or admission.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

mod patch;
pub(crate) use patch::inspect as inspect_patch;

const MAX_SUPPORTS: usize = 16;
const MAX_GRID_POINTS: usize = 8192;
const MAX_PROBES: usize = 8192;

struct Work {
    surface_calls: usize,
    controls: usize,
    construction_estimate: usize,
    preparation_calls: usize,
    plane_projections: usize,
    curve_controls: usize,
    grid_points: usize,
    stored_controls: usize,
}
enum Support {
    Plane {
        origin: P3,
        x: P3,
        y: P3,
    },
    Spline {
        definition: Box<SurfaceDefinition>,
        grid: Vec<([f64; 2], P3)>,
        previous: Option<[f64; 2]>,
    },
}
fn id(row: &Json, name: &str) -> Result<u64, String> {
    row[name]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("shared_support_missing_{name}"))
}
fn plane(step: &StepFile, surface: u64) -> Result<Support, String> {
    let e = entity(step, surface, "PLANE")?;
    let frame = entity(
        step,
        e.reference("position")
            .ok_or("shared_plane_missing_position")?,
        "AXIS2_PLACEMENT_3D",
    )?;
    let origin = point(
        step,
        frame
            .reference("location")
            .ok_or("shared_plane_missing_location")?,
    )?;
    let direction = |name: &str| -> Result<P3, String> {
        let d = entity(
            step,
            frame
                .reference(name)
                .ok_or("shared_plane_requires_explicit_axes")?,
            "DIRECTION",
        )?;
        let values = number_list(d, "direction_ratios", 3)?;
        if values.len() != 3 {
            return Err("shared_plane_requires_3d_directions".into());
        }
        unit([values[0], values[1], values[2]])
    };
    let z = direction("axis")?;
    let y = unit(cross(z, direction("ref_direction")?))?;
    let x = unit(cross(y, z))?;
    Ok(Support::Plane { origin, x, y })
}
fn prepare(step: &StepFile, surface: u64, work: &mut Work) -> Result<Support, String> {
    let source = step.get(surface).ok_or("shared_support_missing_surface")?;
    crate::unit_preflight::shape(source)?;
    if (Entity { inst: source }).is_a("PLANE") {
        return plane(step, surface);
    }
    let source = entity(step, surface, "B_SPLINE_SURFACE")?;
    let rows = source
        .list("control_points_list")
        .ok_or("shared_support_missing_control_rows")?;
    let count = rows.iter().try_fold(0usize, |n, row| {
        n.checked_add(
            row.as_list()
                .ok_or("shared_support_invalid_control_row")?
                .len(),
        )
        .filter(|v| *v <= MAX_CONTROL_POINTS)
        .ok_or("shared_support_control_storage_budget")
    })?;
    work.stored_controls = checked_edge_curve_control_point_total(work.stored_controls, count)?;
    let definition =
        parse_surface_with_preparation_reservation(step, surface, |actual, estimate| {
            if count == 0 || actual != count {
                return Err("shared_support_control_preflight_mismatch".into());
            }
            let calls = estimate / count;
            let next_controls = work
                .controls
                .checked_add(estimate)
                .filter(|n| *n <= MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS)
                .ok_or("shared_support_aggregate_construction_budget")?;
            let next_calls = work
                .surface_calls
                .checked_add(calls)
                .filter(|n| *n <= MAX_SURFACE_EVALUATIONS)
                .ok_or("shared_support_aggregate_preparation_call_budget")?;
            work.controls = next_controls;
            work.surface_calls = next_calls;
            work.construction_estimate += estimate;
            work.preparation_calls += calls;
            Ok(())
        })?;
    let us = axis_seeds(&definition.u_breaks, 4)?;
    let vs = axis_seeds(&definition.v_breaks, 4)?;
    work.grid_points = work
        .grid_points
        .checked_add(us.len() * vs.len())
        .filter(|v| *v <= MAX_GRID_POINTS)
        .ok_or("shared_support_grid_inventory_budget")?;
    let mut grid = Vec::new();
    for &u in &us {
        for &v in &vs {
            let uv = [u, v];
            let p = eval_surface(&definition, uv, &mut work.surface_calls, &mut work.controls)?;
            grid.push((uv, p));
        }
    }
    Ok(Support::Spline {
        definition: Box::new(definition),
        grid,
        previous: None,
    })
}
fn project(
    support: &mut Support,
    target: P3,
    work: &mut Work,
) -> Result<([f64; 2], P3, Json), String> {
    match support {
        Support::Plane { origin, x, y } => {
            work.plane_projections = work
                .plane_projections
                .checked_add(1)
                .filter(|n| *n <= 2 * MAX_PROBES)
                .ok_or("shared_plane_projection_budget")?;
            let d = sub(target, *origin);
            let uv = [dot(d, *x), dot(d, *y)];
            let p = std::array::from_fn(|i| origin[i] + uv[0] * x[i] + uv[1] * y[i]);
            if uv.iter().chain(p.iter()).any(|v| !v.is_finite()) {
                return Err("shared_plane_nonfinite_projection".into());
            }
            Ok((
                uv,
                p,
                json!({"termination":"analytic_support_plane_projection","residualAcceptancePolicyDefined":false,
                "globalMinimumProven":false,"scope":"unbounded_support_plane_only_not_trim_membership","authorizesGeometryAdmission":false}),
            ))
        }
        Support::Spline {
            definition,
            grid,
            previous,
        } => {
            let nearest = grid
                .iter()
                .min_by(|a, b| distance(a.1, target).total_cmp(&distance(b.1, target)))
                .ok_or("shared_support_empty_seed_grid")?
                .0;
            let mut seeds = vec![nearest];
            if let Some(p) = previous {
                if *p != nearest {
                    seeds.push(*p);
                }
            }
            let mut best: Option<ProjectionCandidate> = None;
            let mut starts = 0;
            for seed in seeds {
                let (candidate, count) = multistart_inverse(
                    definition,
                    target,
                    &[seed[0]],
                    &[seed[1]],
                    &mut work.surface_calls,
                    &mut work.controls,
                )?;
                starts += count;
                if best
                    .as_ref()
                    .is_none_or(|b| candidate.residual < b.residual)
                {
                    best = Some(candidate);
                }
            }
            let candidate = best.ok_or("shared_support_no_projection_candidate")?;
            let p = eval_surface(
                definition,
                candidate.uv,
                &mut work.surface_calls,
                &mut work.controls,
            )?;
            *previous = Some(candidate.uv);
            let mut report = candidate.solver_diagnostic();
            report["startCount"] = json!(starts);
            report["seedPolicy"] =
                json!("nearest_actual_coarse_surface_sample_and_previous_canonical_probe");
            Ok((candidate.uv, p, report))
        }
    }
}

fn traversal_view(same: bool, orientation: bool, bound: bool, count: usize) -> (bool, Vec<usize>) {
    let forward = same == (orientation == bound);
    let mut indices = (0..count).collect::<Vec<_>>();
    if !forward {
        indices.reverse();
    }
    (forward, indices)
}

fn inventory_digest(samples: &[(f64, P3)]) -> String {
    let mut h = Sha256::new();
    for (t, p) in samples {
        for value in std::iter::once(t).chain(p.iter()) {
            h.update(value.to_bits().to_le_bytes());
        }
    }
    format!("{:x}", h.finalize())
}

pub(crate) fn inspect(step: &StepFile, topology: Json) -> Result<Json, String> {
    let edges = topology["selectedEdges"]
        .as_array()
        .filter(|e| !e.is_empty() && e.len() <= MAX_FACE_EDGES)
        .ok_or("shared_support_invalid_edge_inventory")?;
    let mut surface_ids = BTreeSet::new();
    for edge in edges {
        let uses = edge["uses"]
            .as_array()
            .filter(|u| u.len() == 2)
            .ok_or("shared_support_requires_two_incident_uses")?;
        for use_ in uses {
            surface_ids.insert(id(use_, "surfaceId")?);
        }
    }
    if surface_ids.len() > MAX_SUPPORTS {
        return Err("shared_support_surface_inventory_budget".into());
    }
    let mut work = Work {
        surface_calls: 0,
        controls: 0,
        construction_estimate: 0,
        preparation_calls: 0,
        plane_projections: 0,
        curve_controls: 0,
        grid_points: 0,
        stored_controls: 0,
    };
    let mut supports = BTreeMap::new();
    for sid in surface_ids {
        supports.insert(sid, prepare(step, sid, &mut work)?);
    }
    let mut rows = Vec::new();
    let mut probes = 0usize;
    let mut curve_storage = 0usize;
    for edge in edges {
        let curve_id = id(edge, "curveGeometryId")?;
        curve_storage = checked_edge_curve_control_point_total(
            curve_storage,
            preflight_curve_control_point_count(step, curve_id)?,
        )?;
        let curve = parse_curve(step, curve_id)?;
        let expected = (curve.breaks.len() - 1)
            .checked_mul(SAMPLES_PER_CURVE_SPAN)
            .and_then(|v| v.checked_add(1))
            .ok_or("shared_curve_probe_overflow")?;
        probes = probes
            .checked_add(expected)
            .filter(|v| *v <= MAX_PROBES)
            .ok_or("shared_curve_probe_budget")?;
        work.curve_controls = work
            .curve_controls
            .checked_add(
                expected
                    .checked_mul(curve.control_point_count)
                    .ok_or("shared_curve_work_overflow")?,
            )
            .filter(|v| *v <= MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS)
            .ok_or("shared_curve_control_budget")?;
        let samples = sample_curve(&curve.curve, &curve.breaks, expected)?;
        if samples.len() != expected {
            return Err("shared_curve_sample_inventory_mismatch".into());
        }
        let same = edge["edgeSameSense"]
            .as_bool()
            .ok_or("shared_curve_unknown_sense")?;
        let start = vertex(
            step,
            id(edge, if same { "startVertexId" } else { "endVertexId" })?,
        )?;
        let end = vertex(
            step,
            id(edge, if same { "endVertexId" } else { "startVertexId" })?,
        )?;
        let endpoint_residuals = [
            distance(start, samples[0].1),
            distance(end, samples[samples.len() - 1].1),
        ];
        if endpoint_residuals.iter().any(|v| !v.is_finite()) {
            return Err("shared_curve_nonfinite_endpoint_residual".into());
        }
        let uses = edge["uses"]
            .as_array()
            .ok_or("shared_support_missing_uses")?;
        let mut points = Vec::new();
        let mut maxima = [0f64; 3];
        let mut residual_failures = [0usize; 2];
        let mut residual_available = [0usize; 2];
        for (index, (parameter, source)) in samples.iter().copied().enumerate() {
            let mut projections = Vec::new();
            let mut support_points = Vec::new();
            for (use_index, use_) in uses.iter().enumerate() {
                let sid = id(use_, "surfaceId")?;
                let (uv, p, solver) = project(
                    supports
                        .get_mut(&sid)
                        .ok_or("shared_support_unprepared_surface")?,
                    source,
                    &mut work,
                )?;
                let residual = distance(source, p);
                if !residual.is_finite() {
                    return Err("shared_support_nonfinite_gap".into());
                }
                maxima[use_index] = maxima[use_index].max(residual);
                residual_available[use_index] += usize::from(
                    solver
                        .get("residualToleranceMet")
                        .is_some_and(|v| v.is_boolean()),
                );
                residual_failures[use_index] +=
                    usize::from(solver.get("residualToleranceMet") == Some(&Json::Bool(false)));
                projections.push(json!({"faceId":use_["faceId"],"surfaceId":sid,"uv":uv,"supportPointSourceUnits":p,
                    "curveToSupportResidualSourceUnits":residual,"solver":solver,"trimMembershipVerified":false}));
                support_points.push(p);
            }
            let gap = distance(support_points[0], support_points[1]);
            if !gap.is_finite() {
                return Err("shared_support_nonfinite_support_gap".into());
            }
            maxima[2] = maxima[2].max(gap);
            points.push(json!({"canonicalSampleIndex":index,"curveParameter":parameter,"sourceCurvePointSourceUnits":source,
                "supportProjections":projections,"supportToSupportGapSourceUnits":gap}));
        }
        let canonical_hash = inventory_digest(&samples);
        let mut views = Vec::new();
        for use_ in uses {
            let orientation = use_["orientation"]
                .as_bool()
                .ok_or("shared_use_unknown_orientation")?;
            let bound = use_["boundOrientation"]
                .as_bool()
                .ok_or("shared_use_unknown_bound_orientation")?;
            let (forward, indices) = traversal_view(same, orientation, bound, samples.len());
            views.push(json!({"faceId":use_["faceId"],"orientedEdgeId":use_["orientedEdgeId"],
                "naturalParameterFollowsBoundTraversal":forward,"canonicalSampleIndices":indices,
                "faceSameSense":use_["faceSameSense"],"scope":"bound_traversal_view_only_not_solid_orientation_proof"}));
        }
        rows.push(json!({"edgeCurveId":edge["edgeCurveId"],"curveGeometryId":curve_id,"nativeCurveDomain":curve.domain,
            "sourceKnotBreaks":curve.breaks,"canonicalSampleCount":samples.len(),"canonicalInventorySha256":canonical_hash,
            "canonicalInventoryHashEncoding":"f64le_parameter_xyz_per_canonical_sample_no_header",
            "parameterScope":"entire_native_curve_domain_sampled_16_per_knot_span_not_a_certified_topological_trim_interval",
            "naturalCurveEndpointToSourceVertexResidualsSourceUnits":endpoint_residuals,"useTraversalViews":views,
            "maximumCurveToFirstSupportResidualSourceUnits":maxima[0],"maximumCurveToSecondSupportResidualSourceUnits":maxima[1],
            "maximumSupportToSupportGapSourceUnits":maxima[2],"numericalResidualThresholdFailureCounts":residual_failures,"numericalResidualThresholdAvailableCounts":residual_available,
            "numericalResidualThresholdUnavailableCounts":residual_available.map(|n|samples.len()-n),"samples":points}));
    }
    let context = topology["ownership"]["selectedRepresentationLengthUnits"]["contextId"]
        .as_u64()
        .ok_or("shared_support_missing_context")?;
    let uncertainty = match crate::source_uncertainty::inspect_context(step, context) {
        Ok(value) => value,
        Err(error) if error == "selected_context_has_no_global_uncertainty_assignment" => {
            json!({"available":false,"reason":error,"usedAsAdmissionTolerance":false})
        }
        Err(error) => return Err(error),
    };
    Ok(
        json!({"schema":"tokito.shared-edge-support-audit.v1","topology":topology,"edges":rows,
        "sourceLengthUncertainty":uncertainty,"work":{"canonicalCurveSamples":probes,"canonicalCurveSampleLimit":MAX_PROBES,
            "curveControlPointEvaluations":work.curve_controls,"curveControlPointLimit":MAX_EDGE_CURVE_CONTROL_POINT_EVALUATIONS,
            "surfaceEvaluationCalls":work.surface_calls,"surfaceEvaluationLimit":MAX_SURFACE_EVALUATIONS,
            "surfaceEvaluationCallsIncludePreparationEstimates":true,"preparationSurfaceCallEstimate":work.preparation_calls,
            "actualExplicitNurbsEvaluationCalls":work.surface_calls-work.preparation_calls,"analyticPlaneProjectionCount":work.plane_projections,
            "budgetChargedSurfaceControlWork":work.controls,"surfaceControlWorkLimit":MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS,
            "constructionAndSeamControlWorkEstimate":work.construction_estimate,"coarseGridPointCount":work.grid_points,
            "surfaceCount":supports.len(),"counterScope":"one_aggregate_budget_across_all_incident_surfaces_including_construction_estimate_no_per_neighbor_reset"},
        "sourceCurveSupportGapResolved":false,"completeTrimCoverageVerified":false,"completeSolidVerified":false,
        "geometryQualified":false,"admitted":false,"exportsProduced":[],
        "scope":"shared_source_curve_samples_and_untrimmed_incident_support_projections_only_no_geometry_repair_or_export"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plane_spline() -> StepFile {
        oxideav_ifc::parse_step(b"ISO-10303-21;HEADER;FILE_DESCRIPTION((''),'2;1');FILE_NAME('','',(''),(''),'','','');FILE_SCHEMA(('AUTOMOTIVE_DESIGN'));ENDSEC;DATA;#1=B_SPLINE_SURFACE_WITH_KNOTS('',1,1,((#2,#3),(#4,#5)),.UNSPECIFIED.,.F.,.F.,.F.,(2,2),(2,2),(0.,1.),(0.,1.),.UNSPECIFIED.);#2=CARTESIAN_POINT('',(0.,0.,0.));#3=CARTESIAN_POINT('',(0.,1.,0.));#4=CARTESIAN_POINT('',(1.,0.,0.));#5=CARTESIAN_POINT('',(1.,1.,0.));ENDSEC;END-ISO-10303-21;").unwrap()
    }
    #[test]
    fn construction_is_reserved_before_kernel_grid_and_global_budgets_do_not_reset() {
        let step = plane_spline();
        let result = parse_surface_with_preparation_reservation(&step, 1, |count, estimate| {
            assert_eq!(count, 4);
            assert_eq!(estimate, 324);
            Err("test_preconstruction_refusal".into())
        });
        assert_eq!(result.err().unwrap(), "test_preconstruction_refusal");
        let mut work = Work {
            surface_calls: 0,
            controls: MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS - 323,
            construction_estimate: 0,
            preparation_calls: 0,
            plane_projections: 0,
            curve_controls: 0,
            grid_points: 0,
            stored_controls: 0,
        };
        assert_eq!(
            prepare(&step, 1, &mut work).err().unwrap(),
            "shared_support_aggregate_construction_budget"
        );
        assert_eq!(
            work.controls,
            MAX_DIAGNOSTIC_CONTROL_POINT_EVALUATIONS - 323
        );
        assert_eq!(work.surface_calls, 0);
        work.controls = 0;
        work.stored_controls = 0;
        work.surface_calls = MAX_SURFACE_EVALUATIONS - 80;
        assert_eq!(
            prepare(&step, 1, &mut work).err().unwrap(),
            "shared_support_aggregate_preparation_call_budget"
        );
        assert_eq!(work.surface_calls, MAX_SURFACE_EVALUATIONS - 80);
        assert_eq!(work.controls, 0);
    }
    #[test]
    fn canonical_encoding_matches_independent_ieee754_little_endian_vector() {
        assert_eq!(
            inventory_digest(&[(0., [1., 2., 3.]), (1., [-0., 4., 5.])]),
            "c2d0f47efb2baf99aab2194838acb0612d48af9ea64915bdcee7e9efd378b888"
        );
        assert_ne!(
            inventory_digest(&[(0., [1., 2., 3.]), (1., [0., 4., 5.])]),
            "c2d0f47efb2baf99aab2194838acb0612d48af9ea64915bdcee7e9efd378b888"
        );
    }
    #[test]
    fn reversed_bound_is_a_view_of_the_same_canonical_inventory() {
        assert_eq!(traversal_view(true, false, false, 3), (true, vec![0, 1, 2]));
        assert_eq!(traversal_view(false, true, false, 3), (true, vec![0, 1, 2]));
        assert_eq!(traversal_view(true, true, false, 3), (false, vec![2, 1, 0]));
        assert_eq!(
            traversal_view(false, false, false, 3),
            (false, vec![2, 1, 0])
        );
    }
    #[test]
    fn plane_projection_preserves_orthogonal_coordinates_and_normal_gap() {
        let mut p = Support::Plane {
            origin: [2., 3., 4.],
            x: [0., 1., 0.],
            y: [-1., 0., 0.],
        };
        let mut work = Work {
            surface_calls: 0,
            controls: 0,
            construction_estimate: 0,
            preparation_calls: 0,
            plane_projections: 0,
            curve_controls: 0,
            grid_points: 0,
            stored_controls: 0,
        };
        let (uv, q, r) = project(&mut p, [1., 5., 7.], &mut work).unwrap();
        assert_eq!(uv, [2., 1.]);
        assert_eq!(q, [1., 5., 4.]);
        assert_eq!(distance(q, [1., 5., 7.]), 3.);
        assert_eq!(r["authorizesGeometryAdmission"], false);
        assert_eq!(work.controls, 0);
        assert!(project(&mut p, [f64::NAN, 0., 0.], &mut work).is_err());
    }
}
