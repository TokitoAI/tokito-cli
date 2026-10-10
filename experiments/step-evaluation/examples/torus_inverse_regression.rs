//! Original analytic experiment for a spindle torus, not a STEP face mesher.
#![forbid(unsafe_code)]
use serde_json::json;
use std::f64::consts::{PI, TAU};

const MAJOR: f64 = 0.100_946_939_893_935;
const MINOR: f64 = 0.150_000_000_000_002;
const MILLIMETRES_PER_UNIT: f64 = 25.4;
const RESIDUAL_TIE_TOLERANCE: f64 = 1e-12;

#[derive(Debug)]
struct Candidate {
    uv: [f64; 2],
    reconstructed: [f64; 3],
    residual: f64,
}
#[derive(Debug)]
struct InverseObservation {
    candidates: [Candidate; 2],
    selected: usize,
    branch_ambiguous: bool,
    azimuth_undefined: bool,
}

fn forward(major: f64, minor: f64, uv: [f64; 2]) -> Result<[f64; 3], &'static str> {
    if !major.is_finite()
        || !minor.is_finite()
        || major <= 0.0
        || minor <= 0.0
        || uv.iter().any(|v| !v.is_finite())
    {
        return Err("nonfinite or nonpositive torus input");
    }
    let radial = major + minor * uv[1].cos();
    let point = [
        radial * uv[0].cos(),
        radial * uv[0].sin(),
        minor * uv[1].sin(),
    ];
    if point.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite torus result");
    }
    Ok(point)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}

fn inverse_observation(
    major: f64,
    minor: f64,
    point: [f64; 3],
) -> Result<InverseObservation, &'static str> {
    if point.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite point");
    }
    forward(major, minor, [0.0, 0.0])?;
    let radius = point[0].hypot(point[1]);
    if !radius.is_finite() {
        return Err("nonfinite cylindrical radius");
    }
    let azimuth = point[1].atan2(point[0]);
    // Each signed radial possibility defines a different meridian-circle angle.
    // The second candidate retains the negative radial branch by rotating u by pi.
    let parameters = [
        [azimuth.rem_euclid(TAU), point[2].atan2(radius - major)],
        [
            (azimuth + PI).rem_euclid(TAU),
            point[2].atan2(-radius - major),
        ],
    ];
    let make = |uv| -> Result<Candidate, &'static str> {
        let reconstructed = forward(major, minor, uv)?;
        let residual = distance(point, reconstructed);
        if !residual.is_finite() {
            return Err("nonfinite inverse residual");
        }
        Ok(Candidate {
            uv,
            reconstructed,
            residual,
        })
    };
    let candidates = [make(parameters[0])?, make(parameters[1])?];
    let azimuth_undefined = radius <= RESIDUAL_TIE_TOLERANCE;
    let branch_ambiguous =
        (candidates[0].residual - candidates[1].residual).abs() <= RESIDUAL_TIE_TOLERANCE;
    let selected = usize::from(candidates[1].residual < candidates[0].residual);
    Ok(InverseObservation {
        candidates,
        selected,
        branch_ambiguous,
        azimuth_undefined,
    })
}

fn report() -> Result<serde_json::Value, &'static str> {
    let mut rows = Vec::new();
    let mut maximum_best_error: f64 = 0.0;
    for (u, v) in [
        (0.0, 0.0),
        (0.7, 0.4),
        (2.2, PI / 2.0),
        (0.0, PI),
        (0.7, 2.8),
        (3.1, 4.1),
        (5.0, 5.8),
    ] {
        let point = forward(MAJOR, MINOR, [u, v])?;
        let observation = inverse_observation(MAJOR, MINOR, point)?;
        let best = observation.candidates[observation.selected].residual;
        maximum_best_error = maximum_best_error.max(best * MILLIMETRES_PER_UNIT);
        rows.push(json!({
            "authoredUv": [u,v], "pointModelUnits": point,
            "outerCandidate": {"uv":observation.candidates[0].uv,"reconstructed":observation.candidates[0].reconstructed,"residualMm":observation.candidates[0].residual*MILLIMETRES_PER_UNIT},
            "innerCandidate": {"uv":observation.candidates[1].uv,"reconstructed":observation.candidates[1].reconstructed,"residualMm":observation.candidates[1].residual*MILLIMETRES_PER_UNIT},
            "selectedCandidateIndex":observation.selected,"branchAmbiguous":observation.branch_ambiguous,"azimuthUndefined":observation.azimuth_undefined
        }));
    }
    let axis_point = [0.0, 0.0, (MINOR * MINOR - MAJOR * MAJOR).sqrt()];
    let pole = inverse_observation(MAJOR, MINOR, axis_point)?;
    Ok(json!({
        "schema":"tokito.analytic-torus-inverse-regression.v1","admitted":false,"exportsProduced":[],
        "radiiModelUnits":{"majorMagnitude":MAJOR,"minor":MINOR},"millimetresPerModelUnit":MILLIMETRES_PER_UNIT,
        "radiusProvenance":"57.step surface #3871 magnitude; mathematical examples, not parsed STEP target evidence",
        "sampleCount":rows.len(),"samples":rows,"maximumSelectedRoundTripErrorMm":maximum_best_error,
        "axisPoleObservation":{"pointModelUnits":axis_point,"branchAmbiguous":pole.branch_ambiguous,"azimuthUndefined":pole.azimuth_undefined,"bestResidualMm":pole.candidates[pole.selected].residual*MILLIMETRES_PER_UNIT},
        "trimDomainSupportVerified":false,"normalOrientationVerified":false,
        "scope":"Analytic forward/inverse residuals only. Candidate selection requires source trim domain, continuity, pole handling and topology validation before face meshing. No upstream implementation is modified."
    }))
}
fn main() {
    match report() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            println!("{}", json!({"admitted":false,"error":error}));
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inner_equator_requires_second_branch() {
        let point = [MAJOR - MINOR, 0.0, 0.0];
        let result = inverse_observation(MAJOR, MINOR, point).unwrap();
        assert_eq!(result.selected, 1);
        assert!(!result.branch_ambiguous);
        assert!(result.candidates[1].residual * MILLIMETRES_PER_UNIT < 1e-12);
        let first_error = result.candidates[0].residual * MILLIMETRES_PER_UNIT;
        assert!((first_error - 2.491_895_453_388_203_4).abs() < 1e-12);
    }
    #[test]
    fn outer_equator_and_finite_samples_round_trip() {
        for uv in [
            [0., 0.],
            [0.7, 0.4],
            [2.2, PI / 2.],
            [0., PI],
            [0.7, 2.8],
            [3.1, 4.1],
            [5., 5.8],
        ] {
            let point = forward(MAJOR, MINOR, uv).unwrap();
            let result = inverse_observation(MAJOR, MINOR, point).unwrap();
            assert!(result.candidates[result.selected].residual * MILLIMETRES_PER_UNIT < 1e-12);
        }
        assert_eq!(
            inverse_observation(MAJOR, MINOR, [MAJOR + MINOR, 0., 0.])
                .unwrap()
                .selected,
            0
        );
    }
    #[test]
    fn axis_pole_exposes_ambiguity_instead_of_claiming_unique_uv() {
        let point = [0., 0., (MINOR * MINOR - MAJOR * MAJOR).sqrt()];
        let result = inverse_observation(MAJOR, MINOR, point).unwrap();
        assert!(result.branch_ambiguous && result.azimuth_undefined);
        assert!(result.candidates[result.selected].residual < 1e-12);
    }
    #[test]
    fn invalid_values_reject() {
        assert!(inverse_observation(MAJOR, MINOR, [f64::NAN, 0., 0.]).is_err());
        assert!(inverse_observation(f64::INFINITY, MINOR, [0., 0., 0.]).is_err());
        assert!(inverse_observation(-MAJOR, MINOR, [0., 0., 0.]).is_err());
        assert!(forward(MAJOR, MINOR, [0., f64::INFINITY]).is_err());
    }
    #[test]
    fn negative_major_surface_requires_uv_reparameterization() {
        for [u, v] in [[0., 0.], [0.7, 0.4], [2.2, PI / 2.], [0., PI]] {
            let radial = -MAJOR + MINOR * v.cos();
            let signed_point = [radial * u.cos(), radial * u.sin(), MINOR * v.sin()];
            let equivalent = forward(MAJOR, MINOR, [u + PI, PI - v]).unwrap();
            assert!(distance(signed_point, equivalent) < 1e-14);
        }
    }
}
