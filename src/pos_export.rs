//! Narrow, independently compared placement-position exports.
//!
//! This is a manufacturing-position projection, separate from the frozen raw
//! placement inventory. It supports bounded CSV and ASCII formats with
//! absolute or KiCad-observed auxiliary drill-file origin semantics.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{EngineError, PhysicalBoardDocument};

const MM_PER_INCH: f64 = 25.4;
const MAX_POS_ASCII_FIELD_BYTES: usize = 1024;
const MAX_POS_ASCII_OUTPUT_BYTES: usize = 32 * 1024 * 1024;

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

#[derive(Debug, Clone, PartialEq)]
pub struct PositionAsciiExport {
    pub ascii: String,
    pub source_sha256: String,
    pub units: PositionUnits,
    pub side: PositionSide,
    pub origin: PositionExportOrigin,
    pub row_count: usize,
}

#[derive(Debug)]
struct PositionRow {
    side_order: u8,
    reference: String,
    value: String,
    package: String,
    x: f64,
    y: f64,
    rotation: f64,
    side: &'static str,
}

#[derive(Debug)]
struct PositionRows {
    rows: Vec<PositionRow>,
    source_sha256: String,
    units: PositionUnits,
    side: PositionSide,
    origin: PositionExportOrigin,
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
        let projection = self.project_position_rows(options)?;
        let row_count = projection.rows.len();
        let mut csv = String::from("Ref,Val,Package,PosX,PosY,Rot,Side\n");
        for row in &projection.rows {
            csv.push_str(&format!(
                "{},{},{},{:.6},{:.6},{:.6},{}",
                csv_field(&row.reference),
                csv_field(&row.value),
                csv_field(&row.package),
                row.x,
                row.y,
                row.rotation,
                row.side
            ));
            csv.push('\n');
        }
        Ok(PositionExport {
            csv,
            source_sha256: projection.source_sha256,
            units: projection.units,
            side: projection.side,
            origin: projection.origin,
            row_count,
        })
    }

    /// Export a KiCad-style whitespace position layout from the same
    /// canonical identities, filters, origin and coordinate transform as CSV.
    pub fn export_pos_ascii(
        &self,
        options: PositionExportOptions,
    ) -> Result<PositionAsciiExport, EngineError> {
        let projection = self.project_position_rows(options)?;
        let row_count = projection.rows.len();
        let ascii = serialize_pos_ascii(&projection.rows, options.units, options.side)?;
        Ok(PositionAsciiExport {
            ascii,
            source_sha256: projection.source_sha256,
            units: projection.units,
            side: projection.side,
            origin: projection.origin,
            row_count,
        })
    }

    fn project_position_rows(
        &self,
        options: PositionExportOptions,
    ) -> Result<PositionRows, EngineError> {
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

        let mut rows = Vec::<PositionRow>::new();
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
            let side = if footprint.side == "front" {
                "top"
            } else {
                "bottom"
            };
            rows.push(PositionRow {
                side_order: if footprint.side == "back" { 0 } else { 1 },
                reference: footprint.reference.clone(),
                value: footprint.value.clone(),
                package: package.to_owned(),
                x,
                y,
                rotation: rotation_deg,
                side,
            });
        }

        rows.sort_by(|left, right| {
            left.side_order
                .cmp(&right.side_order)
                .then_with(|| natural_reference_cmp(&left.reference, &right.reference))
        });
        Ok(PositionRows {
            rows,
            source_sha256: self.source_sha256.clone(),
            units: options.units,
            side: options.side,
            origin,
        })
    }
}

fn serialize_pos_ascii(
    rows: &[PositionRow],
    units: PositionUnits,
    side: PositionSide,
) -> Result<String, EngineError> {
    let (ref_width, value_width, package_width) = rows.iter().try_fold(
        (8_usize, 8_usize, 16_usize),
        |(ref_width, value_width, package_width), row| {
            Ok::<_, EngineError>((
                ref_width.max(ascii_field_width("reference", &row.reference)?),
                value_width.max(ascii_field_width("value", &row.value)?),
                package_width.max(ascii_field_width("package", &row.package)?),
            ))
        },
    )?;

    let timestamp = current_utc_timestamp()?;
    let unit_label = match units {
        PositionUnits::Millimetres => "mm",
        PositionUnits::Inches => "inches",
    };
    let side_label = match side {
        PositionSide::Front => "top",
        PositionSide::Back => "bottom",
        PositionSide::Both => "All",
    };
    let header = format!(
        "### Footprint positions - created on {timestamp} ###\n### Printed by Tokito CLI version {}\n## Unit = {unit_label}, Angle = deg.\n## Side : {side_label}\n",
        env!("CARGO_PKG_VERSION")
    );
    let table_header = format!(
        "{:<ref_width$}  {:<value_width$}  {:<package_width$}  {:>9}  {:>9}  {:>8}  Side\n",
        "# Ref", "Val", "Package", "PosX", "PosY", "Rot"
    );

    let mut output_bytes = header
        .len()
        .checked_add(table_header.len())
        .and_then(|length| length.checked_add("## End\n".len()))
        .ok_or_else(|| EngineError::new("POS ASCII output size calculation overflowed"))?;
    for row in rows {
        let row_bytes = [
            padded_ascii_field_bytes(&row.reference, ref_width),
            2,
            padded_ascii_field_bytes(&row.value, value_width),
            2,
            padded_ascii_field_bytes(&row.package, package_width),
            2,
            format!("{:>9.4}", row.x).len(),
            2,
            format!("{:>9.4}", row.y).len(),
            2,
            format!("{:>8.4}", row.rotation).len(),
            2,
            row.side.len(),
            1,
        ]
        .into_iter()
        .try_fold(0_usize, checked_ascii_size_add)?;
        output_bytes = checked_ascii_size_add(output_bytes, row_bytes)?;
        check_ascii_output_size(output_bytes)?;
    }
    check_ascii_output_size(output_bytes)?;

    let mut output = String::new();
    output.try_reserve_exact(output_bytes).map_err(|_| {
        EngineError::new("POS ASCII output could not reserve its checked buffer size")
    })?;
    output.push_str(&header);
    output.push_str(&table_header);
    for row in rows {
        push_ascii_field(&mut output, &row.reference, ref_width);
        output.push_str("  ");
        push_ascii_field(&mut output, &row.value, value_width);
        output.push_str("  ");
        push_ascii_field(&mut output, &row.package, package_width);
        output.push_str("  ");
        writeln!(
            &mut output,
            "{:>9.4}  {:>9.4}  {:>8.4}  {}",
            row.x, row.y, row.rotation, row.side
        )
        .map_err(|_| EngineError::new("could not serialize POS ASCII numeric fields"))?;
    }
    output.push_str("## End\n");
    debug_assert_eq!(output.len(), output_bytes);
    Ok(output)
}

fn ascii_field_width(label: &str, value: &str) -> Result<usize, EngineError> {
    if value.len() > MAX_POS_ASCII_FIELD_BYTES {
        return Err(EngineError::new(format!(
            "POS ASCII export refused {label} field longer than the {MAX_POS_ASCII_FIELD_BYTES}-byte safety limit"
        )));
    }
    if value.chars().any(is_unsafe_ascii_format_character) {
        return Err(EngineError::new(format!(
            "POS ASCII export refused control, line-separator, or bidi-format characters in the {label} field; use CSV for this text"
        )));
    }
    Ok(value.chars().count())
}

fn is_unsafe_ascii_format_character(character: char) -> bool {
    let codepoint = u32::from(character);
    character.is_control()
        || matches!(
            codepoint,
            0x00AD
                | 0x034F
                | 0x061C
                | 0x200B..=0x200F
                | 0x2028..=0x202E
                | 0x2060..=0x206F
                | 0xFEFF
                | 0xFFF9..=0xFFFB
                | 0xE0001
                | 0xE0020..=0xE007F
        )
}

fn checked_ascii_size_add(total: usize, next: usize) -> Result<usize, EngineError> {
    total
        .checked_add(next)
        .ok_or_else(|| EngineError::new("POS ASCII output size calculation overflowed"))
}

fn check_ascii_output_size(bytes: usize) -> Result<(), EngineError> {
    if bytes > MAX_POS_ASCII_OUTPUT_BYTES {
        return Err(EngineError::new(format!(
            "POS ASCII output exceeds the {MAX_POS_ASCII_OUTPUT_BYTES}-byte safety limit"
        )));
    }
    Ok(())
}

fn push_ascii_field(output: &mut String, value: &str, width: usize) {
    for character in value.chars() {
        output.push(if character == ' ' { '_' } else { character });
    }
    for _ in value.chars().count()..width {
        output.push(' ');
    }
}

fn padded_ascii_field_bytes(value: &str, width: usize) -> usize {
    value.len() + width.saturating_sub(value.chars().count())
}

fn current_utc_timestamp() -> Result<String, EngineError> {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EngineError::new("system clock is before the Unix epoch"))?
        .as_secs();
    let days = i64::try_from(seconds / 86_400)
        .map_err(|_| EngineError::new("system clock is outside the supported UTC date range"))?;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_date_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}"
    ))
}

fn civil_date_from_days(unix_days: i64) -> (i64, i64, i64) {
    let shifted_days = unix_days + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
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

    fn ascii_test_row(reference: &str, value: &str, package: &str) -> PositionRow {
        PositionRow {
            side_order: 1,
            reference: reference.to_owned(),
            value: value.to_owned(),
            package: package.to_owned(),
            x: 5.0,
            y: -5.0,
            rotation: 90.0,
            side: "top",
        }
    }

    #[test]
    fn ascii_timestamp_calendar_conversion_handles_epoch_leap_and_pre_epoch_days() {
        assert_eq!(civil_date_from_days(0), (1970, 1, 1));
        assert_eq!(civil_date_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_date_from_days(10_957), (2000, 1, 1));
        assert_eq!(civil_date_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn ascii_export_uses_kicad_style_columns_and_rejects_unrepresentable_fields() {
        let output = serialize_pos_ascii(
            &[ascii_test_row("R 1", "1 kOhm", "R 0603")],
            PositionUnits::Millimetres,
            PositionSide::Both,
        )
        .unwrap();
        assert!(output.starts_with("### Footprint positions - created on 20"));
        assert!(output.contains("### Printed by Tokito CLI version "));
        assert!(output.contains("## Unit = mm, Angle = deg.\n## Side : All\n"));
        let lines = output.lines().collect::<Vec<_>>();
        assert!(lines[4].starts_with("# Ref"));
        assert!(lines[4].contains("Package"));
        assert!(lines[4].ends_with("Side"));
        assert_eq!(
            lines[5].split_whitespace().collect::<Vec<_>>(),
            [
                "R_1", "1_kOhm", "R_0603", "5.0000", "-5.0000", "90.0000", "top"
            ]
        );
        assert!(output.ends_with("## End\n"));

        let invalid = ascii_test_row("R1", "value\ninjected", "R_0603");
        let error = serialize_pos_ascii(&[invalid], PositionUnits::Millimetres, PositionSide::Both)
            .unwrap_err();
        assert!(error.to_string().contains("control"));

        let bidi = ascii_test_row("R1", "value\u{202e}spoof", "R_0603");
        let error = serialize_pos_ascii(&[bidi], PositionUnits::Millimetres, PositionSide::Both)
            .unwrap_err();
        assert!(error.to_string().contains("bidi-format"));

        let unicode = ascii_test_row("R1", "1kΩ@100MHz", "R_0603");
        let output =
            serialize_pos_ascii(&[unicode], PositionUnits::Millimetres, PositionSide::Both)
                .unwrap();
        assert!(output.contains("1kΩ@100MHz"));

        let overlong = ascii_test_row("R1", &"x".repeat(MAX_POS_ASCII_FIELD_BYTES + 1), "R_0603");
        let error =
            serialize_pos_ascii(&[overlong], PositionUnits::Millimetres, PositionSide::Both)
                .unwrap_err();
        assert!(error.to_string().contains("field longer than"));
    }

    #[test]
    fn ascii_export_rejects_output_budget_exhaustion_and_size_overflow() {
        assert!(check_ascii_output_size(MAX_POS_ASCII_OUTPUT_BYTES).is_ok());
        let error = check_ascii_output_size(MAX_POS_ASCII_OUTPUT_BYTES + 1).unwrap_err();
        assert!(error.to_string().contains("safety limit"));
        assert!(checked_ascii_size_add(usize::MAX, 1).is_err());
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
