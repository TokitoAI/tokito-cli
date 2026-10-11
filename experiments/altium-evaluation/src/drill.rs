//! Explicit-format Excellon source inventory. Does not authorize physical holes.
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn coordinate(text: &str, digits: (usize, usize), lz: bool, scale: f64) -> Result<f64, String> {
    let (sign, text) = if let Some(v) = text.strip_prefix('-') {
        (-1., v)
    } else {
        (1., text)
    };
    if text.is_empty() || text.len() > 16 {
        return Err("invalid_coordinate".into());
    }
    let value = if text.contains('.') {
        if text.bytes().filter(|b| *b == b'.').count() != 1
            || !text.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        {
            return Err("invalid_decimal_coordinate".into());
        }
        text.parse::<f64>()
            .map_err(|_| "invalid_decimal_coordinate")?
    } else {
        let width = digits.0 + digits.1;
        if text.len() > width || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err("invalid_fixed_coordinate".into());
        }
        // Excellon LZ retains leading zeros; missing trailing digits are padded.
        let padded = if lz {
            format!("{text:0<width$}")
        } else {
            format!("{text:0>width$}")
        };
        padded
            .parse::<u64>()
            .map_err(|_| "invalid_fixed_coordinate")? as f64
            / 10f64.powi(digits.1 as i32)
    };
    let result = value * scale * sign;
    if !result.is_finite() || result.abs() > 10_000. {
        return Err("coordinate_outside_budget".into());
    }
    Ok(result)
}
fn axes(
    line: &str,
    previous: [Option<f64>; 2],
    digits: (usize, usize),
    lz: bool,
    scale: f64,
) -> Result<[f64; 2], String> {
    let mut p = previous;
    let mut seen = [false; 2];
    let mut at = 0;
    while at < line.len() {
        let axis = match line.as_bytes()[at] {
            b'X' => 0,
            b'Y' => 1,
            _ => return Err("unsupported_coordinate_command".into()),
        };
        if seen[axis] {
            return Err("duplicate_axis".into());
        }
        seen[axis] = true;
        at += 1;
        let start = at;
        while at < line.len() && !matches!(line.as_bytes()[at], b'X' | b'Y') {
            at += 1;
        }
        p[axis] = Some(coordinate(&line[start..at], digits, lz, scale)?);
    }
    if !seen.iter().any(|v| *v) {
        return Err("empty_coordinate_command".into());
    }
    Ok([
        p[0].ok_or("missing_initial_x")?,
        p[1].ok_or("missing_initial_y")?,
    ])
}

pub fn inspect(text: &str) -> Result<Value, String> {
    if text.len() > 1024 * 1024 || !text.is_ascii() {
        return Err("drill_input_budget_or_encoding".into());
    }
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("M48") {
        return Err("missing_excellon_header".into());
    }
    let mut digits = None;
    let mut units = None;
    let mut plating = None;
    let mut tools = BTreeMap::new();
    let mut in_header = true;
    let mut ended = false;
    let mut selected = None;
    let mut position = [None, None];
    let mut holes = Vec::new();
    let mut slots = Vec::new();
    for (index, raw) in lines.enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.len() > 1024 || index > 100_000 {
            return Err("drill_record_budget".into());
        }
        if ended {
            return Err("data_after_end".into());
        }
        if let Some(format) = line.strip_prefix(";FILE_FORMAT=") {
            if !in_header || digits.is_some() {
                return Err("duplicate_or_late_format".into());
            }
            let (a, b) = format.split_once(':').ok_or("invalid_format")?;
            let a = a.parse::<usize>().map_err(|_| "invalid_format")?;
            let b = b.parse::<usize>().map_err(|_| "invalid_format")?;
            if !(1..=4).contains(&a) || !(1..=6).contains(&b) || a + b > 8 {
                return Err("unsupported_format".into());
            }
            digits = Some((a, b));
            continue;
        }
        if let Some(kind) = line.strip_prefix(";TYPE=") {
            if !in_header || plating.is_some() || !matches!(kind, "PLATED" | "NON_PLATED") {
                return Err("invalid_plating_declaration".into());
            }
            plating = Some(kind.to_owned());
            continue;
        }
        if line.starts_with(';') {
            continue;
        }
        if in_header {
            if matches!(line, "INCH,LZ" | "INCH,TZ" | "METRIC,LZ" | "METRIC,TZ") {
                if units.is_some() {
                    return Err("duplicate_units".into());
                }
                units = Some((
                    if line.starts_with("INCH") { 25.4 } else { 1. },
                    line.ends_with("LZ"),
                ));
            } else if line == "%" {
                if digits.is_none() || units.is_none() || plating.is_none() || tools.is_empty() {
                    return Err("incomplete_drill_header".into());
                }
                in_header = false;
            } else if let Some(tool) = line.strip_prefix('T') {
                let (prefix, diameter) = tool.split_once('C').ok_or("missing_tool_diameter")?;
                let end = prefix
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(prefix.len());
                let id = prefix[..end].parse::<u16>().map_err(|_| "invalid_tool")?;
                if id == 0 || !matches!(&prefix[end..], "" | "F00S00") {
                    return Err("unsupported_tool_parameters".into());
                }
                if !diameter.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                    return Err("invalid_tool_diameter".into());
                }
                let d = diameter
                    .parse::<f64>()
                    .map_err(|_| "invalid_tool_diameter")?;
                if !d.is_finite()
                    || d <= 0.
                    || d > 100.
                    || tools.len() >= 256
                    || tools.insert(id, d).is_some()
                {
                    return Err("invalid_or_duplicate_tool".into());
                }
            } else {
                return Err(format!("unsupported_header_command:{line}"));
            }
        } else {
            let format = digits.unwrap();
            let (scale, lz) = units.unwrap();
            if matches!(line, "G90" | "G05") {
                continue;
            }
            if line == "M30" {
                ended = true;
                continue;
            }
            if let Some(tool) = line.strip_prefix('T') {
                let id = tool.parse::<u16>().map_err(|_| "invalid_tool_selection")?;
                if !tools.contains_key(&id) {
                    return Err("undefined_tool".into());
                }
                selected = Some(id);
                continue;
            }
            let tool = selected.ok_or("coordinate_without_tool")?;
            let diameter = tools[&tool] * scale;
            if let Some((start, end)) = line.split_once("G85") {
                let a = axes(start, position, format, lz, scale)?;
                let b = axes(end, [Some(a[0]), Some(a[1])], format, lz, scale)?;
                if a == b {
                    return Err("zero_length_slot".into());
                }
                slots.push(json!({"startMm":a,"endMm":b,"diameterMm":diameter,"tool":tool,"sourceLine":index+2}));
                position = [Some(b[0]), Some(b[1])];
            } else {
                let p = axes(line, position, format, lz, scale)?;
                holes.push(
                    json!({"positionMm":p,"diameterMm":diameter,"tool":tool,"sourceLine":index+2}),
                );
                position = [Some(p[0]), Some(p[1])];
            }
            if holes.len() + slots.len() > 100_000 {
                return Err("drill_feature_budget".into());
            }
        }
    }
    if in_header || !ended {
        return Err("truncated_drill_program".into());
    }
    let (scale, lz) = units.unwrap();
    let tool_rows: Vec<_> = tools
        .iter()
        .map(|(id, d)| json!({"tool":id,"diameterMm":d*scale}))
        .collect();
    Ok(
        json!({"schema":"tokito.excellon-source-inventory.v1","lengthUnit":"mm","sourceUnitScaleToMm":scale,"sourceDigits":digits.unwrap(),"leadingZerosRetained":lz,"coordinateMode":"absolute","platingClaim":plating,"tools":tool_rows,"holes":holes,"slots":slots,"geometryQualified":false,"sourceBoardFrameVerified":false,"physicalFitVerified":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str = "M48\n;FILE_FORMAT=2:5\nINCH,LZ\n;TYPE=PLATED\nT1F00S00C0.03500\n%\nT01\n";
    #[test]
    fn retained_leading_zeros_and_modal_axes_are_metric() {
        let r = inspect(&format!("{HEADER}X0041Y-0136\nX-0041\nM30\n")).unwrap();
        for (row, expected) in [(0, [10.414, -34.544]), (1, [-10.414, -34.544])] {
            for (axis, value) in expected.iter().enumerate() {
                assert!(
                    (r["holes"][row]["positionMm"][axis].as_f64().unwrap() - value).abs() < 1e-12
                );
            }
        }
    }
    #[test]
    fn tz_and_decimal_coordinates_remain_distinct() {
        assert!((coordinate("41", (2, 5), false, 25.4).unwrap() - 0.010414).abs() < 1e-12);
        assert!((coordinate("0.41", (2, 5), false, 25.4).unwrap() - 10.414).abs() < 1e-12);
    }
    #[test]
    fn canned_slots_retain_both_endpoints() {
        let r = inspect(&format!(
            "{HEADER}G90\nG05\nX00143Y-0136613G85Y-0138187\nM30\n"
        ))
        .unwrap();
        assert_eq!(r["slots"].as_array().unwrap().len(), 1);
        assert!(r["holes"].as_array().unwrap().is_empty());
    }
    #[test]
    fn missing_and_ambiguous_format_and_commands_refuse() {
        for tail in [
            "X1\nM30",
            "G91\nM30",
            "X1Y2X3\nM30",
            "X1Y2",
            "M30\nX1Y2",
            "T02\nM30",
        ] {
            assert!(inspect(&format!("{HEADER}{tail}")).is_err(), "{tail}");
        }
        assert!(inspect(&HEADER.replace(";FILE_FORMAT=2:5\n", "")).is_err());
        assert!(inspect(&HEADER.replace("INCH,LZ", "INCH,LZ\nMETRIC,TZ")).is_err());
    }
}
