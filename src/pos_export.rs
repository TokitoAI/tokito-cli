//! Narrow, independently compared CSV position export.
//!
//! This is a manufacturing-position projection, separate from the frozen raw
//! placement inventory. It supports a bounded CSV subset with absolute or
//! KiCad 10.0.5-observed auxiliary drill-file origin semantics.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::{EngineError, PhysicalBoardDocument};

const MM_PER_INCH: f64 = 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSide {
    Front,
    Back,
    Both,
}

impl PositionSide {
    fn includes(self, side: &str) -> bool {
        match self {
            Self::Front => side == "front",
            Self::Back => side == "back",
            Self::Both => matches!(side, "front" | "back"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionUnits {
    Millimetres,
    Inches,
}

/// Coordinate origin used for the emitted position values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionExportOrigin {
    /// Source-board absolute coordinates (the historical default).
    Absolute,
    /// KiCad drill-file origin. `declared` is false when the board has no
    /// auxiliary origin, for which the observed CLI behavior is `(0, 0)`.
    DrillFile {
        x_mm: f64,
        y_mm: f64,
        declared: bool,
    },
}

impl PositionUnits {
    fn value(self, millimetres: f64) -> f64 {
        match self {
            Self::Millimetres => millimetres,
            Self::Inches => millimetres / MM_PER_INCH,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Millimetres => "mm",
            Self::Inches => "in",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PositionExportOptions {
    pub side: PositionSide,
    pub units: PositionUnits,
    pub bottom_negate_x: bool,
    pub smd_only: bool,
    pub exclude_fp_th: bool,
    pub exclude_dnp: bool,
    pub use_drill_file_origin: bool,
}

impl Default for PositionExportOptions {
    fn default() -> Self {
        Self {
            side: PositionSide::Both,
            units: PositionUnits::Inches,
            bottom_negate_x: false,
            smd_only: false,
            exclude_fp_th: false,
            exclude_dnp: false,
            use_drill_file_origin: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionExport {
    pub csv: String,
    pub source_sha256: String,
    pub units: PositionUnits,
    pub side: PositionSide,
    pub origin: PositionExportOrigin,
    pub row_count: usize,
}

#[derive(Default)]
struct FootprintClass {
    smd: bool,
    has_through_hole_pad: bool,
    has_unclassified_pad: bool,
}

impl PhysicalBoardDocument {
    /// Export KiCad-style CSV position rows using the board's absolute origin.
    ///
    /// This method does not change or replace [`Self::placements_csv`], which
    /// preserves source-frame coordinates and identity inventory semantics.
    pub fn export_pos_csv(
        &self,
        options: PositionExportOptions,
    ) -> Result<PositionExport, EngineError> {
        // Keep the legacy absolute-origin path byte-for-byte stable. Only
        // inspect the source auxiliary origin when explicitly requested.
        let origin = if options.use_drill_file_origin {
            drill_file_origin(self)?
        } else {
            PositionExportOrigin::Absolute
        };
        let mut classes = BTreeMap::<usize, FootprintClass>::new();
        for component in &self.engineering.components {
            if component.source.kind != "footprint" {
                continue;
            }
            let class = classes.entry(component.source.ordinal).or_default();
            class.smd = component
                .attributes
                .iter()
                .any(|attribute| attribute.eq_ignore_ascii_case("smd"));
        }
        for pad in &self.engineering.pads {
            let Some(component_ordinal) = pad.source.component_ordinal else {
                continue;
            };
            let Some(class) = classes.get_mut(&component_ordinal) else {
                continue;
            };
            match pad.pad_type.as_deref() {
                Some("thru_hole" | "np_thru_hole") => class.has_through_hole_pad = true,
                Some("smd") => {}
                _ => class.has_unclassified_pad = true,
            }
        }

        let mut rows = Vec::<(u8, String, String)>::new();
        let mut references = BTreeSet::new();
        let mut component_ordinals = BTreeMap::new();
        for component in &self.engineering.components {
            if component.source.kind == "footprint" {
                if let Some(uuid) = component.source.source_uuid.as_deref() {
                    if component_ordinals
                        .insert(uuid, component.source.ordinal)
                        .is_some()
                    {
                        return Err(EngineError::new(
                            "POS export stopped because a component UUID is duplicated",
                        ));
                    }
                }
            }
        }
        for footprint in &self.footprints {
            if !matches!(footprint.side.as_str(), "front" | "back") {
                return Err(EngineError::new(format!(
                    "POS export stopped because footprint {:?} has an unsupported copper side",
                    footprint.reference
                )));
            }
            // KiCad excludes this authored attribute from POS output by default.
            if footprint.excluded_from_pos_files {
                continue;
            }
            if options.exclude_dnp && footprint.dnp {
                continue;
            }
            if !options.side.includes(&footprint.side) {
                continue;
            }

            let component_ordinal = component_ordinals
                .get(footprint.uuid.as_str())
                .copied()
                .ok_or_else(|| {
                    EngineError::new(format!(
                        "POS export cannot bind footprint {:?} to canonical component attributes",
                        footprint.reference
                    ))
                })?;
            let class = classes.get(&component_ordinal).ok_or_else(|| {
                EngineError::new(format!(
                    "POS export has no component classification for {:?}",
                    footprint.reference
                ))
            })?;
            if options.smd_only && !class.smd {
                continue;
            }
            if options.exclude_fp_th {
                if class.has_unclassified_pad {
                    return Err(EngineError::new(format!(
                        "POS --exclude-fp-th cannot classify every pad on footprint {:?}",
                        footprint.reference
                    )));
                }
                if class.has_through_hole_pad {
                    continue;
                }
            }

            let x_mm = footprint.x_mm.ok_or_else(|| {
                EngineError::new(format!(
                    "POS export stopped because footprint {:?} has invalid X placement",
                    footprint.reference
                ))
            })?;
            let y_mm = footprint.y_mm.ok_or_else(|| {
                EngineError::new(format!(
                    "POS export stopped because footprint {:?} has invalid Y placement",
                    footprint.reference
                ))
            })?;
            let rotation_deg = footprint.rotation_deg.ok_or_else(|| {
                EngineError::new(format!(
                    "POS export stopped because footprint {:?} has invalid rotation",
                    footprint.reference
                ))
            })?;
            if ![x_mm, y_mm, rotation_deg]
                .iter()
                .all(|value| value.is_finite())
            {
                return Err(EngineError::new(format!(
                    "POS export stopped because footprint {:?} has a non-finite pose",
                    footprint.reference
                )));
            }
            if !footprint.reference_is_placeholder && !references.insert(&footprint.reference) {
                return Err(EngineError::new(format!(
                    "POS export stopped because reference {:?} is duplicated",
                    footprint.reference
                )));
            }

            let (position_x_mm, position_y_mm) = translated_position(x_mm, y_mm, origin)?;
            let mut x = options.units.value(position_x_mm);
            if options.bottom_negate_x && footprint.side == "back" {
                x = -x;
            }
            let y = options.units.value(position_y_mm);
            let package = footprint
                .library_id
                .rsplit_once(':')
                .map_or(footprint.library_id.as_str(), |(_, package)| package);
            let row = format!(
                "{},{},{},{:.6},{:.6},{:.6},{}",
                csv_field(&footprint.reference),
                csv_field(&footprint.value),
                csv_field(package),
                x,
                y,
                rotation_deg,
                if footprint.side == "front" {
                    "top"
                } else {
                    "bottom"
                }
            );
            rows.push((
                if footprint.side == "back" { 0 } else { 1 },
                footprint.reference.clone(),
                row,
            ));
        }

        rows.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| natural_reference_cmp(&left.1, &right.1))
        });
        let row_count = rows.len();
        let mut csv = String::from("Ref,Val,Package,PosX,PosY,Rot,Side\n");
        for (_, _, row) in rows {
            csv.push_str(&row);
            csv.push('\n');
        }

        Ok(PositionExport {
            csv,
            source_sha256: self.source_sha256.clone(),
            units: options.units,
            side: options.side,
            origin,
            row_count,
        })
    }
}

fn translated_position(
    x_mm: f64,
    y_mm: f64,
    origin: PositionExportOrigin,
) -> Result<(f64, f64), EngineError> {
    match origin {
        PositionExportOrigin::Absolute => Ok((x_mm, -y_mm)),
        PositionExportOrigin::DrillFile {
            x_mm: origin_x,
            y_mm: origin_y,
            ..
        } => {
            let x = x_mm - origin_x;
            let y = origin_y - y_mm;
            if !x.is_finite() || !y.is_finite() {
                return Err(EngineError::new(
                    "POS --use-drill-file-origin produced non-finite translated coordinates",
                ));
            }
            Ok((x, y))
        }
    }
}

fn drill_file_origin(board: &PhysicalBoardDocument) -> Result<PositionExportOrigin, EngineError> {
    // KiCad stores the auxiliary axis origin under the board's direct setup
    // child. Search the full source tree so misplaced or duplicate declarations
    // cannot be silently ignored, but only accept that exact parent node.
    let board_setups: Vec<&crate::SourceNode> = board.source_document.children("setup").collect();
    let mut stack = vec![(&board.source_document, None::<&crate::SourceNode>)];
    let mut declarations = Vec::new();
    while let Some((node, parent)) = stack.pop() {
        if node.head() == Some("aux_axis_origin") {
            let is_board_setup = parent.is_some_and(|parent| {
                board_setups
                    .iter()
                    .any(|setup| std::ptr::eq(*setup, parent))
            });
            declarations.push((node, is_board_setup));
            if declarations.len() > 1 {
                return Err(EngineError::new(
                    "POS --use-drill-file-origin refused duplicate or ambiguous auxiliary origins",
                ));
            }
        }
        if let Some(items) = node.items() {
            stack.extend(items.iter().skip(1).map(|child| (child, Some(node))));
        }
    }

    let Some((declaration, is_board_setup)) = declarations.first().copied() else {
        return Ok(PositionExportOrigin::DrillFile {
            x_mm: 0.0,
            y_mm: 0.0,
            declared: false,
        });
    };
    if !is_board_setup || board_setups.len() != 1 {
        return Err(EngineError::new(
            "POS --use-drill-file-origin refused a malformed or ambiguous auxiliary origin outside the board's direct setup",
        ));
    }
    if declaration.items().is_none_or(|items| items.len() != 3) {
        return Err(EngineError::new(
            "POS --use-drill-file-origin refused a malformed auxiliary origin; expected exactly two coordinates",
        ));
    }
    let parse_coordinate = |index: usize, axis: &str| -> Result<f64, EngineError> {
        let value = declaration
            .value_at(index)
            .ok_or_else(|| EngineError::new(format!(
                "POS --use-drill-file-origin refused a malformed auxiliary origin: missing {axis} coordinate"
            )))?
            .parse::<f64>()
            .map_err(|_| EngineError::new(format!(
                "POS --use-drill-file-origin refused a malformed auxiliary origin: invalid {axis} coordinate"
            )))?;
        if !value.is_finite() {
            return Err(EngineError::new(format!(
                "POS --use-drill-file-origin refused a non-finite auxiliary {axis} coordinate"
            )));
        }
        Ok(value)
    };
    let x_mm = parse_coordinate(1, "X")?;
    let y_mm = parse_coordinate(2, "Y")?;
    Ok(PositionExportOrigin::DrillFile {
        x_mm,
        y_mm,
        declared: true,
    })
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn natural_reference_cmp(left: &str, right: &str) -> Ordering {
    let mut left = left.chars().peekable();
    let mut right = right.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left_char), Some(right_char))
                if left_char.is_ascii_digit() && right_char.is_ascii_digit() =>
            {
                let left_digits = take_digit_run(&mut left);
                let right_digits = take_digit_run(&mut right);
                let left_significant = left_digits.trim_start_matches('0');
                let right_significant = right_digits.trim_start_matches('0');
                let numeric_order = left_significant
                    .len()
                    .cmp(&right_significant.len())
                    .then_with(|| left_significant.cmp(right_significant));
                if numeric_order != Ordering::Equal {
                    return numeric_order;
                }
                let zero_padding_order = left_digits.len().cmp(&right_digits.len());
                if zero_padding_order != Ordering::Equal {
                    return zero_padding_order;
                }
            }
            (Some(left_char), Some(right_char)) => {
                let order = left_char
                    .to_ascii_lowercase()
                    .cmp(&right_char.to_ascii_lowercase());
                left.next();
                right.next();
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

fn take_digit_run(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut digits = String::new();
    while chars.peek().is_some_and(char::is_ascii_digit) {
        if let Some(digit) = chars.next() {
            digits.push(digit);
        }
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_kicad_board;
    use std::path::PathBuf;

    fn fixture(relative: &str) -> PhysicalBoardDocument {
        load_kicad_board(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative))
            .expect("load pinned KiCad fixture")
    }

    fn reference(relative: &str) -> String {
        std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative))
            .expect("read pinned KiCad 10.0.5 POS reference")
            .replace("\r\n", "\n")
    }

    fn uno_options() -> PositionExportOptions {
        PositionExportOptions {
            units: PositionUnits::Millimetres,
            ..PositionExportOptions::default()
        }
    }

    fn export_source_text(
        source: String,
        options: PositionExportOptions,
    ) -> Result<PositionExport, EngineError> {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "tokito-cli-pos-origin-{}-{unique}.kicad_pcb",
            std::process::id()
        ));
        std::fs::write(&path, source).unwrap();
        let result = load_kicad_board(&path).and_then(|board| board.export_pos_csv(options));
        std::fs::remove_file(path).unwrap();
        result
    }

    fn drill_origin_options() -> PositionExportOptions {
        PositionExportOptions {
            units: PositionUnits::Millimetres,
            use_drill_file_origin: true,
            ..PositionExportOptions::default()
        }
    }

    #[test]
    fn drill_origin_translation_refuses_overflow_from_finite_inputs() {
        let error = translated_position(
            f64::MAX,
            0.0,
            PositionExportOrigin::DrillFile {
                x_mm: -f64::MAX,
                y_mm: 0.0,
                declared: true,
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("non-finite translated coordinates")
        );
    }

    fn pos_rows_without_empty_reference_order(csv: &str) -> (Vec<&str>, Vec<&str>) {
        let mut named_rows = Vec::new();
        let mut empty_reference_rows = Vec::new();
        for row in csv.lines().skip(1) {
            if row.starts_with("\"\",") {
                empty_reference_rows.push(row);
            } else {
                named_rows.push(row);
            }
        }
        empty_reference_rows.sort_unstable();
        (named_rows, empty_reference_rows)
    }

    #[test]
    fn sparkfun_drill_origin_matches_kicad_10_0_5_reference() {
        let output = fixture(
            "tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb",
        )
        .export_pos_csv(drill_origin_options())
        .unwrap();
        assert_eq!(output.row_count, 17);
        assert_eq!(
            output.origin,
            PositionExportOrigin::DrillFile {
                x_mm: 115.57,
                y_mm: 124.46,
                declared: true,
            }
        );
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/spark-mm-drill-origin.csv")
        );
    }

    #[test]
    fn pico_drill_origin_and_bottom_negation_match_kicad_10_0_5_references() {
        let board = fixture(
            "tests/fixtures/open-source/raspberry-pi-pico-community/RPI-PICO-R3a-PUBLIC - Official.kicad_pcb",
        );
        let output = board.export_pos_csv(drill_origin_options()).unwrap();
        assert_eq!(output.row_count, 57);
        let expected = reference("tests/reference/kicad-cli-10.0.5/pos/pico-mm-drill-origin.csv");
        assert_eq!(
            pos_rows_without_empty_reference_order(&output.csv),
            pos_rows_without_empty_reference_order(&expected),
            "all CSV values and named-reference order match; KiCad orders duplicate empty references differently"
        );
        let bottom_negated = board
            .export_pos_csv(PositionExportOptions {
                bottom_negate_x: true,
                ..drill_origin_options()
            })
            .unwrap();
        let expected = reference(
            "tests/reference/kicad-cli-10.0.5/pos/pico-mm-drill-origin-bottom-negate.csv",
        );
        assert_eq!(
            pos_rows_without_empty_reference_order(&bottom_negated.csv),
            pos_rows_without_empty_reference_order(&expected),
            "all CSV values and named-reference order match; KiCad orders duplicate empty references differently"
        );
    }

    #[test]
    fn absent_auxiliary_origin_uses_absolute_zero_origin() {
        let output =
            fixture("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb")
                .export_pos_csv(drill_origin_options())
                .unwrap();
        assert_eq!(
            output.origin,
            PositionExportOrigin::DrillFile {
                x_mm: 0.0,
                y_mm: 0.0,
                declared: false,
            }
        );
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/uno-mm-both.csv")
        );
    }

    #[test]
    fn malformed_nonfinite_duplicate_and_misplaced_auxiliary_origins_are_refused() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb",
        );
        let source = std::fs::read_to_string(fixture_path).unwrap();
        let declaration = "(aux_axis_origin 115.57 124.46)";

        let malformed = source.replacen(declaration, "(aux_axis_origin 115.57)", 1);
        let error = export_source_text(malformed, drill_origin_options()).unwrap_err();
        assert!(error.to_string().contains("malformed auxiliary origin"));

        let nonfinite = source.replacen(declaration, "(aux_axis_origin nan 124.46)", 1);
        let error = export_source_text(nonfinite, drill_origin_options()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("non-finite auxiliary X coordinate")
        );

        let duplicate = source.replacen(
            declaration,
            "(aux_axis_origin 115.57 124.46)\n\t\t(aux_axis_origin 1 2)",
            1,
        );
        let error = export_source_text(duplicate, drill_origin_options()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate or ambiguous auxiliary origins")
        );

        let mut misplaced = source.replacen(declaration, "", 1);
        let final_close = misplaced.rfind(')').unwrap();
        misplaced.insert_str(final_close, "\n\t(aux_axis_origin 1 2)\n");
        let error = export_source_text(misplaced, drill_origin_options()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("outside the board's direct setup")
        );

        let nested_setup = source.replacen(declaration, "", 1).replacen(
            "\t\t(thickness 1.6)",
            "\t\t(thickness 1.6)\n\t\t(setup (aux_axis_origin 1 2))",
            1,
        );
        let error = export_source_text(nested_setup, drill_origin_options()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("outside the board's direct setup")
        );
    }

    #[test]
    fn uno_both_sides_mm_matches_kicad_10_0_5_csv_reference() {
        let output =
            fixture("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb")
                .export_pos_csv(uno_options())
                .unwrap();
        assert_eq!(output.row_count, 54);
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/uno-mm-both.csv")
        );
    }

    #[test]
    fn uno_smd_filter_matches_kicad_10_0_5_csv_reference() {
        let output =
            fixture("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb")
                .export_pos_csv(PositionExportOptions {
                    smd_only: true,
                    ..uno_options()
                })
                .unwrap();
        assert_eq!(output.row_count, 44);
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/uno-mm-smd.csv")
        );
    }

    #[test]
    fn uno_through_hole_filter_matches_kicad_10_0_5_csv_reference() {
        let output =
            fixture("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb")
                .export_pos_csv(PositionExportOptions {
                    exclude_fp_th: true,
                    ..uno_options()
                })
                .unwrap();
        assert_eq!(output.row_count, 43);
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/uno-mm-exclude-fp-th.csv")
        );
    }

    #[test]
    fn uno_dnp_filter_matches_kicad_10_0_5_csv_reference() {
        let output =
            fixture("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb")
                .export_pos_csv(PositionExportOptions {
                    exclude_dnp: true,
                    ..uno_options()
                })
                .unwrap();
        assert_eq!(output.row_count, 52);
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/uno-mm-exclude-dnp.csv")
        );
    }

    #[test]
    fn uno_inches_match_kicad_10_0_5_csv_reference() {
        let output =
            fixture("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb")
                .export_pos_csv(PositionExportOptions::default())
                .unwrap();
        assert_eq!(
            output.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/uno-in-both.csv")
        );
    }

    #[test]
    fn sparkfun_np_through_hole_pads_match_kicad_exclusion_reference() {
        let board = fixture(
            "tests/fixtures/open-source/sparkfun-default-kicad/SparkFun_Basic_Board_Example.kicad_pcb",
        );
        let baseline = board
            .export_pos_csv(PositionExportOptions {
                units: PositionUnits::Millimetres,
                ..PositionExportOptions::default()
            })
            .unwrap();
        let no_through_hole = board
            .export_pos_csv(PositionExportOptions {
                units: PositionUnits::Millimetres,
                exclude_fp_th: true,
                ..PositionExportOptions::default()
            })
            .unwrap();
        assert_eq!(baseline.row_count, 17);
        assert_eq!(no_through_hole.row_count, 13);
        assert!(baseline.csv.contains("\"ST1\""));
        assert!(!no_through_hole.csv.contains("\"ST1\""));
        assert_eq!(
            baseline.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/spark-mm-both.csv")
        );
        assert_eq!(
            no_through_hole.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/spark-mm-exclude-fp-th.csv")
        );
    }

    #[test]
    fn nxp_side_selection_and_bottom_x_negation_match_kicad_reference() {
        let board = fixture("tests/fixtures/open-source/nxp-mcxa-esc/MR-ESC-MCXA-AM32.kicad_pcb");
        let options = PositionExportOptions {
            units: PositionUnits::Millimetres,
            ..PositionExportOptions::default()
        };
        let both = board.export_pos_csv(options).unwrap();
        let front = board
            .export_pos_csv(PositionExportOptions {
                side: PositionSide::Front,
                ..options
            })
            .unwrap();
        let back = board
            .export_pos_csv(PositionExportOptions {
                side: PositionSide::Back,
                ..options
            })
            .unwrap();
        let mirrored = board
            .export_pos_csv(PositionExportOptions {
                bottom_negate_x: true,
                ..options
            })
            .unwrap();
        assert_eq!(both.row_count, 67);
        assert_eq!(front.row_count, 51);
        assert_eq!(back.row_count, 16);
        assert_eq!(
            both.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/nxp-mm-both.csv")
        );
        assert_eq!(
            front.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/nxp-mm-front.csv")
        );
        assert_eq!(
            back.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/nxp-mm-back.csv")
        );
        assert_eq!(
            mirrored.csv,
            reference("tests/reference/kicad-cli-10.0.5/pos/nxp-mm-bottom-negate.csv")
        );
    }

    #[test]
    fn quote_strings_and_escape_embedded_quotes() {
        assert_eq!(
            csv_field("resistor, \"wide\""),
            "\"resistor, \"\"wide\"\"\""
        );
    }

    #[test]
    fn unsupported_footprint_layer_fails_closed_instead_of_disappearing() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/open-source/arduino-uno-r3-community/Arduino UNO.kicad_pcb");
        let mut source = std::fs::read_to_string(fixture_path).unwrap();
        let footprint_start = source.find("(footprint ").unwrap();
        let layer_start =
            footprint_start + source[footprint_start..].find("(layer \"F.Cu\")").unwrap();
        source.replace_range(
            layer_start..layer_start + "(layer \"F.Cu\")".len(),
            "(layer \"In1.Cu\")",
        );

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let board_path = std::env::temp_dir().join(format!(
            "tokito-cli-pos-unsupported-layer-{}-{unique}.kicad_pcb",
            std::process::id()
        ));
        std::fs::write(&board_path, source).unwrap();
        let error = load_kicad_board(&board_path)
            .unwrap()
            .export_pos_csv(uno_options())
            .unwrap_err();
        assert!(error.to_string().contains("unsupported copper side"));
        std::fs::remove_file(board_path).unwrap();
    }
}
