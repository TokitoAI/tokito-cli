//! Original, bounded two-terminal SMD footprint generation in millimetres.
//!
//! The nominal profile is an independently implemented candidate compatible
//! with KiCad 10.0.5's `R_0603_1608Metric` placement geometry. This module
//! does not claim a qualified land pattern for a particular manufacturer.

use std::fmt::{self, Write};

const MAX_DIMENSION_MM: f64 = 100.0;

#[derive(Debug, Clone, PartialEq)]
pub struct FootprintError(pub String);

impl fmt::Display for FootprintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FootprintError {}

/// A symmetric two-terminal chip footprint. All coordinates and sizes are mm.
#[derive(Debug, Clone, PartialEq)]
pub struct TwoTerminalChipSpec {
    pub name: String,
    pub body_length_mm: f64,
    pub body_width_mm: f64,
    pub pad_pitch_mm: f64,
    pub pad_length_mm: f64,
    pub pad_width_mm: f64,
    pub pad_corner_ratio: f64,
    pub courtyard_length_mm: f64,
    pub courtyard_width_mm: f64,
    pub silk_half_length_mm: f64,
    pub silk_y_mm: f64,
}

impl TwoTerminalChipSpec {
    /// Geometry compared with installed KiCad 10.0.5 R_0603_1608Metric.
    pub fn r0603_nominal() -> Self {
        Self {
            name: "R_0603_1608Metric".into(),
            body_length_mm: 1.6,
            body_width_mm: 0.825,
            pad_pitch_mm: 1.65,
            pad_length_mm: 0.8,
            pad_width_mm: 0.95,
            pad_corner_ratio: 0.25,
            courtyard_length_mm: 2.96,
            courtyard_width_mm: 1.46,
            silk_half_length_mm: 0.237258,
            silk_y_mm: 0.5225,
        }
    }

    pub fn validate(&self) -> Result<(), FootprintError> {
        if self.name.is_empty()
            || self.name.len() > 96
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        {
            return Err(FootprintError(
                "footprint name must contain 1–96 ASCII letters, digits, '_', '-' or '.'".into(),
            ));
        }
        for (label, value) in [
            ("body_length_mm", self.body_length_mm),
            ("body_width_mm", self.body_width_mm),
            ("pad_pitch_mm", self.pad_pitch_mm),
            ("pad_length_mm", self.pad_length_mm),
            ("pad_width_mm", self.pad_width_mm),
            ("courtyard_length_mm", self.courtyard_length_mm),
            ("courtyard_width_mm", self.courtyard_width_mm),
            ("silk_half_length_mm", self.silk_half_length_mm),
            ("silk_y_mm", self.silk_y_mm),
        ] {
            if !value.is_finite() || !(0.000001..=MAX_DIMENSION_MM).contains(&value) {
                return Err(FootprintError(format!(
                    "{label} must be finite and within [0.000001, 100] mm"
                )));
            }
        }
        if !self.pad_corner_ratio.is_finite() || !(0.0..=0.5).contains(&self.pad_corner_ratio) {
            return Err(FootprintError(
                "pad_corner_ratio must be finite and within [0, 0.5]".into(),
            ));
        }
        if self.pad_pitch_mm <= self.pad_length_mm {
            return Err(FootprintError("pads must have a positive gap".into()));
        }
        if self.courtyard_length_mm / 2.0 <= self.pad_pitch_mm / 2.0 + self.pad_length_mm / 2.0
            || self.courtyard_width_mm / 2.0 <= self.pad_width_mm / 2.0
            || self.courtyard_length_mm <= self.body_length_mm
            || self.courtyard_width_mm <= self.body_width_mm
            || self.courtyard_width_mm / 2.0 <= self.silk_y_mm + 0.06
        {
            return Err(FootprintError(
                "courtyard must enclose body, pads and silkscreen".into(),
            ));
        }
        if self.silk_half_length_mm >= self.body_length_mm / 2.0
            || self.silk_y_mm <= self.body_width_mm / 2.0
        {
            return Err(FootprintError(
                "silkscreen must sit outside the body and have a bounded length".into(),
            ));
        }
        Ok(())
    }

    /// Deterministic, UTF-8 KiCad `.kicad_mod` text. No model is asserted.
    pub fn to_kicad_mod(&self) -> Result<String, FootprintError> {
        self.validate()?;
        let mut out = String::with_capacity(1800);
        writeln!(out, "(footprint \"{}\"", self.name).unwrap();
        writeln!(out, "  (version 20260206)").unwrap();
        writeln!(out, "  (generator \"tokito-cli\")").unwrap();
        writeln!(out, "  (layer \"F.Cu\")").unwrap();
        writeln!(out, "  (attr smd)").unwrap();
        // A reference visible outside the footprint; the value is on Fab.
        writeln!(out, "  (property \"Reference\" \"REF**\" (at 0 -1.43 0) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))").unwrap();
        writeln!(out, "  (property \"Value\" \"{}\" (at 0 1.43 0) (layer \"F.Fab\") (effects (font (size 1 1) (thickness 0.15))))", self.name).unwrap();
        for y in [-self.silk_y_mm, self.silk_y_mm] {
            writeln!(out, "  (fp_line (start {} {}) (end {} {}) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\"))", number(-self.silk_half_length_mm), number(y), number(self.silk_half_length_mm), number(y)).unwrap();
        }
        rect(
            &mut out,
            self.courtyard_length_mm,
            self.courtyard_width_mm,
            0.05,
            "F.CrtYd",
        );
        rect(
            &mut out,
            self.body_length_mm,
            self.body_width_mm,
            0.1,
            "F.Fab",
        );
        for (number_label, x) in [
            ("1", -self.pad_pitch_mm / 2.0),
            ("2", self.pad_pitch_mm / 2.0),
        ] {
            writeln!(out, "  (pad \"{}\" smd roundrect (at {} 0) (size {} {}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (roundrect_rratio {}))", number_label, number(x), number(self.pad_length_mm), number(self.pad_width_mm), number(self.pad_corner_ratio)).unwrap();
        }
        out.push_str(")\n");
        Ok(out)
    }
}

fn rect(out: &mut String, length: f64, width: f64, stroke: f64, layer: &str) {
    writeln!(out, "  (fp_rect (start {} {}) (end {} {}) (stroke (width {}) (type solid)) (fill no) (layer \"{}\"))", number(-length / 2.0), number(-width / 2.0), number(length / 2.0), number(width / 2.0), number(stroke), layer).unwrap();
}

fn number(value: f64) -> String {
    // Nine places preserve the pinned 0603 geometry and submicron inputs.
    let rounded = format!("{value:.9}");
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".into()
    } else {
        trimmed.into()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn installed_geometry_reference_matches_pinned_digest_when_present() {
        let path = std::path::Path::new(
            "C:/Program Files/KiCad/10.0/share/kicad/footprints/Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod",
        );
        if path.is_file() {
            let bytes = std::fs::read(path).expect("read optional installed KiCad reference");
            assert_eq!(
                crate::sha256_hex(&bytes),
                "03fc7902b2661df01b4d828fdb6eab9eddf974e7130ed2b98892607387a50c4b",
                "the optional installed KiCad geometry reference changed"
            );
        }
    }
}
