//! Unit normalization of explicit source properties, without pose composition.
use serde_json::{json, Value};

const MAX_PROPERTY_BYTES: usize = 128;
// Operational input limits, not engineering dimensions or acceptance tolerances.
const MAX_ABS_MILS: f64 = 1_000_000.0;
const MAX_ABS_DEGREES: f64 = 360_000.0;

fn text<'a>(properties: &'a Value, key: &str) -> Result<&'a str, String> {
    let value = properties
        .get(key)
        .ok_or_else(|| format!("missing_property:{key}"))?
        .as_str()
        .ok_or_else(|| format!("non_string_property:{key}"))?;
    if value.len() > MAX_PROPERTY_BYTES {
        return Err(format!("property_byte_limit_exceeded:{key}"));
    }
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("empty_property:{key}"));
    }
    Ok(value)
}

fn number(value: &str, key: &str, maximum: f64) -> Result<f64, String> {
    let value = value
        .parse::<f64>()
        .map_err(|_| format!("invalid_number:{key}"))?;
    if !value.is_finite() {
        return Err(format!("nonfinite_number:{key}"));
    }
    if value.abs() > maximum {
        return Err(format!("number_limit_exceeded:{key}"));
    }
    Ok(value)
}

fn millimeters(properties: &Value, key: &str) -> Result<f64, String> {
    let value = text(properties, key)?;
    let mils = value
        .strip_suffix("mil")
        .ok_or_else(|| format!("unsupported_length_unit:{key}"))?;
    Ok(number(mils.trim(), key, MAX_ABS_MILS)? * 0.0254)
}

fn degrees(properties: &Value, key: &str) -> Result<f64, String> {
    number(text(properties, key)?, key, MAX_ABS_DEGREES)
}

pub fn inspect(component: &Value, body: &Value) -> Result<Value, String> {
    let side = match text(component, "LAYER")? {
        "TOP" => "top",
        "BOTTOM" => "bottom",
        _ => return Err("unsupported_component_side:LAYER".into()),
    };
    let projection = match text(body, "BODYPROJECTION")? {
        "0" => "top",
        "1" => "bottom",
        _ => return Err("unsupported_body_projection:BODYPROJECTION".into()),
    };
    Ok(json!({"schema":"tokito.altium-source-pose-inputs.v1",
        "numericInputsResolved":true,"lengthUnit":"mm","angleUnit":"degree",
        "coordinateFrame":"source-altium-XY; source-Z-offset; no handedness conversion",
        "component":{"positionMm":[millimeters(component,"X")?,millimeters(component,"Y")?],
            "side":side,"rotationDegrees":degrees(component,"ROTATION")?},
        "body":{"positionMm":[millimeters(body,"MODEL.2D.X")?,millimeters(body,"MODEL.2D.Y")?],
            "zOffsetMm":millimeters(body,"MODEL.3D.DZ")?,
            "rotation2dDegrees":degrees(body,"MODEL.2D.ROTATION")?,
            "rotation3dDegrees":[degrees(body,"MODEL.3D.ROTX")?,degrees(body,"MODEL.3D.ROTY")?,degrees(body,"MODEL.3D.ROTZ")?],
            "projection":projection},
        "modelsTablePoseApplied":false,"worldTransform":Value::Null,"poseVerified":false,
        "inputLimits":{"propertyBytes":MAX_PROPERTY_BYTES,"absoluteMils":MAX_ABS_MILS,
            "absoluteDegrees":MAX_ABS_DEGREES}}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inputs() -> (Value, Value) {
        (
            json!({"X":"100mil","Y":"-50mil","ROTATION":" 2.70000000000000E+0002","LAYER":"TOP"}),
            json!({"MODEL.2D.X":"100mil","MODEL.2D.Y":"-50mil","MODEL.3D.DZ":"19.685mil",
                "MODEL.2D.ROTATION":"0.000","MODEL.3D.ROTX":"90.000","MODEL.3D.ROTY":"0.000",
                "MODEL.3D.ROTZ":"360.000","BODYPROJECTION":"0"}),
        )
    }
    #[test]
    fn source_mils_and_degrees_convert_without_composing_pose() {
        let (component, body) = inputs();
        let result = inspect(&component, &body).unwrap();
        assert_eq!(result["component"]["positionMm"], json!([2.54, -1.27]));
        assert!((result["body"]["zOffsetMm"].as_f64().unwrap() - 0.499999).abs() < 1e-12);
        assert_eq!(result["component"]["rotationDegrees"], 270.0);
        assert_eq!(
            result["body"]["rotation3dDegrees"],
            json!([90.0, 0.0, 360.0])
        );
        assert!(result["worldTransform"].is_null());
        assert_eq!(result["poseVerified"], false);
        assert_eq!(result["modelsTablePoseApplied"], false);
    }
    #[test]
    fn sides_and_projection_are_independent_source_inputs() {
        let (mut component, mut body) = inputs();
        for (side, label) in [("TOP", "top"), ("BOTTOM", "bottom")] {
            component["LAYER"] = json!(side);
            for (projection, p_label) in [("0", "top"), ("1", "bottom")] {
                body["BODYPROJECTION"] = json!(projection);
                let result = inspect(&component, &body).unwrap();
                assert_eq!(result["component"]["side"], label);
                assert_eq!(result["body"]["projection"], p_label);
            }
        }
        component["LAYER"] = json!("MECHANICAL13");
        assert!(inspect(&component, &body)
            .unwrap_err()
            .contains("unsupported_component_side"));
        component["LAYER"] = json!("TOP");
        body["BODYPROJECTION"] = json!("2");
        assert!(inspect(&component, &body)
            .unwrap_err()
            .contains("unsupported_body_projection"));
    }
    #[test]
    fn incomplete_unsupported_and_oversized_properties_refuse() {
        let (component, body) = inputs();
        for (value, error) in [
            (json!("1mm"), "unsupported_length_unit"),
            (json!("NaNmil"), "nonfinite_number"),
            (json!("infmil"), "nonfinite_number"),
            (json!("1000001mil"), "number_limit_exceeded"),
            (json!(""), "empty_property"),
            (json!(1), "non_string_property"),
            (json!("1".repeat(129)), "property_byte_limit_exceeded"),
        ] {
            let mut c = component.clone();
            c["X"] = value;
            assert!(inspect(&c, &body).unwrap_err().contains(error));
        }
        let mut c = component.clone();
        c.as_object_mut().unwrap().remove("X");
        assert!(inspect(&c, &body)
            .unwrap_err()
            .contains("missing_property:X"));
        let mut b = body.clone();
        b["MODEL.3D.ROTX"] = json!("NaN");
        assert!(inspect(&component, &b)
            .unwrap_err()
            .contains("nonfinite_number"));
        b["MODEL.3D.ROTX"] = json!("360001");
        assert!(inspect(&component, &b)
            .unwrap_err()
            .contains("number_limit_exceeded"));
    }
}
