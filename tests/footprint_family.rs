use std::fs;
use std::path::Path;
use tokito_cli::footprint_family::TwoTerminalChipSpec;

const REFERENCE_PATH: &str = "C:/Program Files/KiCad/10.0/share/kicad/footprints/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod";
// Installed KiCad 10.0.5 reference SHA-256:
// 03fc7902b2661df01b4d828fdb6eab9eddf974e7130ed2b98892607387a50c4b

#[derive(Debug, PartialEq)]
struct Pad {
    number: String,
    at: Vec<f64>,
    size: Vec<f64>,
    layers: Vec<String>,
    corner_ratio: f64,
}

fn top_level_forms<'a>(input: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut forms = Vec::new();
    for (start, _) in input.match_indices(prefix) {
        // The `pad` and `fp_rect` tokens are only used for those geometry
        // records in this pair of documents, never within an escaped string.
        let bytes = input.as_bytes();
        let mut depth = 0_i32;
        let mut quoted = false;
        let mut escape = false;
        for end in start..input.len() {
            let b = bytes[end];
            if quoted {
                if escape {
                    escape = false;
                } else if b == b'\\' {
                    escape = true;
                } else if b == b'"' {
                    quoted = false;
                }
            } else if b == b'"' {
                quoted = true;
            } else if b == b'(' {
                depth += 1;
            } else if b == b')' {
                depth -= 1;
                if depth == 0 {
                    forms.push(&input[start..=end]);
                    break;
                }
            }
        }
    }
    forms
}

fn values(form: &str, key: &str) -> Vec<String> {
    let start = form
        .find(key)
        .unwrap_or_else(|| panic!("missing {key}: {form}"))
        + key.len();
    let end = form[start..].find(')').expect("field close") + start;
    form[start..end]
        .split_ascii_whitespace()
        .map(|v| v.trim_matches('"').to_owned())
        .collect()
}

fn coordinates(form: &str, key: &str) -> Vec<f64> {
    values(form, key)
        .into_iter()
        .map(|v| v.parse().expect("number"))
        .collect()
}

fn pads(input: &str) -> Vec<Pad> {
    top_level_forms(input, "(pad ")
        .iter()
        .map(|form| {
            let number = form[6..].split('"').next().expect("pad number").to_owned();
            Pad {
                number,
                at: coordinates(form, "(at "),
                size: coordinates(form, "(size "),
                layers: values(form, "(layers "),
                corner_ratio: coordinates(form, "(roundrect_rratio ")[0],
            }
        })
        .collect()
}

fn rectangle(input: &str, layer: &str) -> (Vec<f64>, Vec<f64>) {
    let form = top_level_forms(input, "(fp_rect")
        .into_iter()
        .find(|form| values(form, "(layer ") == [layer])
        .expect("rectangle layer");
    (coordinates(form, "(start "), coordinates(form, "(end "))
}

fn silk_segments(input: &str) -> Vec<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    top_level_forms(input, "(fp_line")
        .into_iter()
        .filter(|form| values(form, "(layer ") == ["F.SilkS"])
        .map(|form| {
            (
                coordinates(form, "(start "),
                coordinates(form, "(end "),
                coordinates(form, "(width "),
            )
        })
        .collect()
}

#[test]
fn nominal_0603_matches_independently_read_reference_geometry() {
    let generated = TwoTerminalChipSpec::r0603_nominal()
        .to_kicad_mod()
        .expect("valid nominal profile");
    let expected_pads = vec![
        Pad {
            number: "1".into(),
            at: vec![-0.825, 0.0],
            size: vec![0.8, 0.95],
            layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
            corner_ratio: 0.25,
        },
        Pad {
            number: "2".into(),
            at: vec![0.825, 0.0],
            size: vec![0.8, 0.95],
            layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
            corner_ratio: 0.25,
        },
    ];
    assert_eq!(pads(&generated), expected_pads);
    assert_eq!(
        rectangle(&generated, "F.Fab"),
        (vec![-0.8, -0.4125], vec![0.8, 0.4125])
    );
    assert_eq!(
        rectangle(&generated, "F.CrtYd"),
        (vec![-1.48, -0.73], vec![1.48, 0.73])
    );
    assert_eq!(
        silk_segments(&generated),
        vec![
            (
                vec![-0.237258, -0.5225],
                vec![0.237258, -0.5225],
                vec![0.12]
            ),
            (vec![-0.237258, 0.5225], vec![0.237258, 0.5225], vec![0.12]),
        ]
    );

    // Installed KiCad data is optional for other developers/CI. On the
    // project machine, compare independently parsed geometry to the pinned
    // 10.0.5 candidate, not its prose, model link, or generator text.
    if Path::new(REFERENCE_PATH).is_file() {
        let reference = fs::read_to_string(REFERENCE_PATH).expect("installed reference");
        assert_eq!(pads(&generated), pads(&reference));
        assert_eq!(
            rectangle(&generated, "F.Fab"),
            rectangle(&reference, "F.Fab")
        );
        assert_eq!(
            rectangle(&generated, "F.CrtYd"),
            rectangle(&reference, "F.CrtYd")
        );
        assert_eq!(silk_segments(&generated), silk_segments(&reference));
    }
}

#[test]
fn serializer_is_deterministic_and_rejects_malformed_specs() {
    let valid = TwoTerminalChipSpec::r0603_nominal();
    assert_eq!(valid.to_kicad_mod().unwrap(), valid.to_kicad_mod().unwrap());
    let mut invalid = valid.clone();
    invalid.name = "R_0603\") (pad \"9\"".into();
    assert!(invalid.to_kicad_mod().is_err());
    invalid = valid.clone();
    invalid.pad_pitch_mm = f64::NAN;
    assert!(invalid.to_kicad_mod().is_err());
    invalid = valid.clone();
    invalid.pad_pitch_mm = 0.7;
    assert!(invalid.to_kicad_mod().is_err());
    invalid = valid.clone();
    invalid.courtyard_width_mm = 0.5;
    assert!(invalid.to_kicad_mod().is_err());
    invalid = valid.clone();
    invalid.pad_corner_ratio = 0.75;
    assert!(invalid.to_kicad_mod().is_err());
    invalid = valid;
    invalid.body_width_mm = f64::INFINITY;
    assert!(invalid.to_kicad_mod().is_err());
}
