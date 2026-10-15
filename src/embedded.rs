//! Bounded resolution of KiCad board-embedded model assets.
//! A resolved payload is source bytes, not geometry-qualified CAD.

use crate::document::{ModelReference, SourceLocator};
use crate::model_resolver::ModelAssetFormat;
use crate::{PhysicalBoardDocument, SourceNode, sha256_hex};
use std::io::Read;

const EMBEDDED_URI_PREFIX: &str = "kicad-embed://";
const MAX_EMBEDDED_NAME_BYTES: usize = 255;
const MAX_COMPRESSED_MODEL_BYTES: usize = 256 * 1024 * 1024;
const ZSTD_FRAME_OVERHEAD_BUDGET: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedChecksumAlgorithm {
    Mmh3X64_128,
    Mmh3X64_128Legacy,
    Sha256,
}

impl EmbeddedChecksumAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mmh3X64_128 => "mmh3-x64-128",
            Self::Mmh3X64_128Legacy => "mmh3-x64-128-legacy-v1",
            Self::Sha256 => "sha256-legacy",
        }
    }
}

/// Embedded bytes whose stored checksum has been verified. This does not
/// certify geometry, units, placement, or suitability for a digital twin.
#[derive(Debug, Clone)]
pub struct EmbeddedModelAsset {
    pub name: String,
    pub source: SourceLocator,
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub checksum_algorithm: EmbeddedChecksumAlgorithm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedModelError {
    InvalidUri,
    Missing,
    Ambiguous,
    MetadataConflict,
    UnsupportedType,
    MissingData,
    MalformedData,
    TooLarge,
    DecompressionFailed,
    ChecksumMissing,
    ChecksumMismatch,
    UnsupportedFormat,
}

impl EmbeddedModelError {
    pub fn diagnostic(self) -> &'static str {
        match self {
            Self::InvalidUri => "Embedded model URI is invalid",
            Self::Missing => "Embedded model is absent from the board source",
            Self::Ambiguous => "Multiple embedded models match the exact URI",
            Self::MetadataConflict => {
                "Component-local and board-level embedded model metadata disagree"
            }
            Self::UnsupportedType => "Embedded file is not declared as a model",
            Self::MissingData => "Embedded model has no payload",
            Self::MalformedData => "Embedded model payload is malformed",
            Self::TooLarge => "Embedded model exceeds the configured byte limit",
            Self::DecompressionFailed => "Embedded model Zstandard frame is invalid",
            Self::ChecksumMissing => "Embedded model has no supported checksum",
            Self::ChecksumMismatch => "Embedded model checksum does not match its payload",
            Self::UnsupportedFormat => "Embedded model file extension is unsupported",
        }
    }
}

pub(crate) fn parse_embedded_uri(path: &str) -> Result<&str, EmbeddedModelError> {
    let name = path
        .strip_prefix(EMBEDDED_URI_PREFIX)
        .ok_or(EmbeddedModelError::InvalidUri)?;
    if name.is_empty()
        || name.len() > MAX_EMBEDDED_NAME_BYTES
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '?' | '#'))
    {
        return Err(EmbeddedModelError::InvalidUri);
    }
    Ok(name)
}

pub(crate) fn format_from_name(name: &str) -> Result<ModelAssetFormat, EmbeddedModelError> {
    match name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("step" | "stp") => Ok(ModelAssetFormat::Step),
        Some("wrl" | "vrml") => Ok(ModelAssetFormat::Wrl),
        Some("glb") => Ok(ModelAssetFormat::Glb),
        Some("gltf") => Ok(ModelAssetFormat::Gltf),
        _ => Err(EmbeddedModelError::UnsupportedFormat),
    }
}

pub(crate) fn resolve_model(
    board: &PhysicalBoardDocument,
    model: &ModelReference,
    max_bytes: usize,
) -> Result<EmbeddedModelAsset, EmbeddedModelError> {
    let name = parse_embedded_uri(
        model
            .path
            .as_deref()
            .ok_or(EmbeddedModelError::InvalidUri)?,
    )?;
    format_from_name(name)?;
    let files = &board.engineering().embedded_files;
    let local_matches = model
        .source
        .component_ordinal
        .map(|ordinal| {
            files
                .iter()
                .filter(|entry| {
                    entry.source.component_ordinal == Some(ordinal)
                        && entry.name.as_deref() == Some(name)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let board_matches = files
        .iter()
        .filter(|entry| {
            entry.source.component_ordinal.is_none() && entry.name.as_deref() == Some(name)
        })
        .collect::<Vec<_>>();
    let entry = match local_matches.as_slice() {
        [local] if local.has_data => *local,
        [local] => {
            if local
                .file_type
                .as_deref()
                .is_some_and(|kind| kind != "model")
            {
                return Err(EmbeddedModelError::UnsupportedType);
            }
            let board_entry = match board_matches.as_slice() {
                [entry] if entry.has_data => *entry,
                [] => return Err(EmbeddedModelError::MissingData),
                [_] => return Err(EmbeddedModelError::MissingData),
                _ => return Err(EmbeddedModelError::Ambiguous),
            };
            if board_entry.file_type.as_deref() != Some("model") {
                return Err(EmbeddedModelError::UnsupportedType);
            }
            if local.checksum.as_deref().is_some_and(|checksum| {
                !board_entry
                    .checksum
                    .as_deref()
                    .is_some_and(|board_checksum| checksum.eq_ignore_ascii_case(board_checksum))
            }) {
                return Err(EmbeddedModelError::MetadataConflict);
            }
            board_entry
        }
        [] => match board_matches.as_slice() {
            [entry] => *entry,
            [] => return Err(EmbeddedModelError::Missing),
            _ => return Err(EmbeddedModelError::Ambiguous),
        },
        _ => return Err(EmbeddedModelError::Ambiguous),
    };
    if entry.file_type.as_deref() != Some("model") {
        return Err(EmbeddedModelError::UnsupportedType);
    }
    if !entry.has_data {
        return Err(EmbeddedModelError::MissingData);
    }
    let file = board
        .source_node(&entry.source)
        .ok_or(EmbeddedModelError::MalformedData)?;
    let data = file.child("data").ok_or(EmbeddedModelError::MissingData)?;
    let compressed_budget = max_bytes
        .saturating_add(ZSTD_FRAME_OVERHEAD_BUDGET)
        .min(MAX_COMPRESSED_MODEL_BYTES);
    let encoded = extract_pipe_payload(data, compressed_budget)?;
    let compressed = decode_base64(&encoded, compressed_budget)?;
    let bytes = decompress_single_frame(&compressed, max_bytes)?;
    let checksum = entry
        .checksum
        .as_deref()
        .ok_or(EmbeddedModelError::ChecksumMissing)?;
    let checksum_algorithm = verify_checksum(checksum, &bytes)?;
    Ok(EmbeddedModelAsset {
        name: name.to_owned(),
        source: entry.source.clone(),
        sha256: sha256_hex(&bytes),
        bytes,
        checksum_algorithm,
    })
}

fn extract_pipe_payload(
    data: &SourceNode,
    max_compressed_bytes: usize,
) -> Result<Vec<u8>, EmbeddedModelError> {
    let max_encoded_bytes = max_compressed_bytes
        .checked_mul(4)
        .and_then(|value| value.checked_div(3))
        .and_then(|value| value.checked_add(8))
        .ok_or(EmbeddedModelError::TooLarge)?;
    let mut payload = Vec::new();
    let mut opened = false;
    let mut closed = false;
    for item in data
        .items()
        .ok_or(EmbeddedModelError::MalformedData)?
        .iter()
        .skip(1)
    {
        let atom = item.atom().ok_or(EmbeddedModelError::MalformedData)?;
        for byte in atom.bytes() {
            match byte {
                b'|' if !opened => opened = true,
                b'|' if opened && !closed => closed = true,
                b'|' => return Err(EmbeddedModelError::MalformedData),
                byte if opened && !closed => {
                    if !matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'=')
                    {
                        return Err(EmbeddedModelError::MalformedData);
                    }
                    payload.push(byte);
                    if payload.len() > max_encoded_bytes {
                        return Err(EmbeddedModelError::TooLarge);
                    }
                }
                _ => return Err(EmbeddedModelError::MalformedData),
            }
        }
    }
    if !opened || !closed || payload.is_empty() {
        return Err(EmbeddedModelError::MalformedData);
    }
    Ok(payload)
}

fn decode_base64(encoded: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EmbeddedModelError> {
    if encoded.is_empty() || encoded.len() % 4 != 0 {
        return Err(EmbeddedModelError::MalformedData);
    }
    let capacity = encoded
        .len()
        .checked_div(4)
        .and_then(|value| value.checked_mul(3))
        .ok_or(EmbeddedModelError::TooLarge)?;
    if capacity > max_output_bytes.saturating_add(2) {
        return Err(EmbeddedModelError::TooLarge);
    }
    let mut output = Vec::with_capacity(capacity.min(max_output_bytes));
    for (index, quartet) in encoded.chunks_exact(4).enumerate() {
        let final_quartet = index + 1 == encoded.len() / 4;
        let a = base64_value(quartet[0]).ok_or(EmbeddedModelError::MalformedData)?;
        let b = base64_value(quartet[1]).ok_or(EmbeddedModelError::MalformedData)?;
        output.push((a << 2) | (b >> 4));
        match (quartet[2], quartet[3]) {
            (b'=', b'=') if final_quartet && b & 0x0f == 0 => {}
            (b'=', _) => return Err(EmbeddedModelError::MalformedData),
            (c, b'=') if final_quartet => {
                let c = base64_value(c).ok_or(EmbeddedModelError::MalformedData)?;
                if c & 0x03 != 0 {
                    return Err(EmbeddedModelError::MalformedData);
                }
                output.push((b << 4) | (c >> 2));
            }
            (c, d) => {
                let c = base64_value(c).ok_or(EmbeddedModelError::MalformedData)?;
                let d = base64_value(d).ok_or(EmbeddedModelError::MalformedData)?;
                output.push((b << 4) | (c >> 2));
                output.push((c << 6) | d);
            }
        }
        if output.len() > max_output_bytes {
            return Err(EmbeddedModelError::TooLarge);
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decompress_single_frame(
    compressed: &[u8],
    max_output_bytes: usize,
) -> Result<Vec<u8>, EmbeddedModelError> {
    use ruzstd::decoding::StreamingDecoder;
    use std::io::Cursor;
    let mut decoder = StreamingDecoder::new(Cursor::new(compressed))
        .map_err(|_| EmbeddedModelError::DecompressionFailed)?;
    let declared = decoder.decoder.content_size();
    if declared == 0 {
        return Err(EmbeddedModelError::DecompressionFailed);
    }
    if declared > max_output_bytes as u64 {
        return Err(EmbeddedModelError::TooLarge);
    }
    let mut output = Vec::with_capacity((declared as usize).min(max_output_bytes));
    (&mut decoder)
        .take(max_output_bytes as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|_| EmbeddedModelError::DecompressionFailed)?;
    if output.len() > max_output_bytes {
        return Err(EmbeddedModelError::TooLarge);
    }
    let (_, frame_decoder) = decoder.into_parts();
    if frame_decoder.bytes_read_from_source() != compressed.len() as u64 {
        return Err(EmbeddedModelError::MalformedData);
    }
    if output.is_empty() {
        return Err(EmbeddedModelError::MalformedData);
    }
    Ok(output)
}

fn verify_checksum(
    stored: &str,
    bytes: &[u8],
) -> Result<EmbeddedChecksumAlgorithm, EmbeddedModelError> {
    if stored.len() == 32 && stored.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let current = mmh3_x64_128(bytes, 0xABBA_2345, false);
        let legacy = mmh3_x64_128(bytes, 0xABBA_2345, true);
        if stored.eq_ignore_ascii_case(&current) {
            return Ok(EmbeddedChecksumAlgorithm::Mmh3X64_128);
        }
        if stored.eq_ignore_ascii_case(&legacy) {
            return Ok(EmbeddedChecksumAlgorithm::Mmh3X64_128Legacy);
        }
        return Err(EmbeddedModelError::ChecksumMismatch);
    }
    if stored.len() == 64 && stored.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return if stored.eq_ignore_ascii_case(&sha256_hex(bytes)) {
            Ok(EmbeddedChecksumAlgorithm::Sha256)
        } else {
            Err(EmbeddedModelError::ChecksumMismatch)
        };
    }
    Err(EmbeddedModelError::ChecksumMissing)
}

fn mmh3_x64_128(bytes: &[u8], seed: u32, legacy_v1: bool) -> String {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;
    let mut h1 = u64::from(seed);
    let mut h2 = u64::from(seed);
    for block in bytes.chunks_exact(16) {
        let mut k1 = u64::from_le_bytes(block[..8].try_into().expect("eight-byte block"));
        let mut k2 = u64::from_le_bytes(block[8..16].try_into().expect("eight-byte block"));
        k1 = k1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        h1 ^= k1;
        h1 = h1
            .rotate_left(27)
            .wrapping_add(h2)
            .wrapping_mul(5)
            .wrapping_add(0x52dc_e729);
        k2 = k2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        h2 ^= k2;
        h2 = h2
            .rotate_left(31)
            .wrapping_add(h1)
            .wrapping_mul(5)
            .wrapping_add(0x3849_5ab5);
    }
    let remainder = bytes.len() % 16;
    let tail = &bytes[bytes.len() - remainder..];
    // The pre-fix KiCad writer rounded its final input to a four-byte
    // boundary and included the padded length in the digest. The tail length
    // is therefore the padded length modulo 16, not the original remainder.
    let padding = if legacy_v1 && remainder > 0 {
        4 - (remainder + 4) % 4
    } else {
        0
    };
    let effective_len = bytes.len().saturating_add(padding);
    let tail_len = if legacy_v1 {
        effective_len % 16
    } else {
        remainder
    };
    let mut block = [0u8; 16];
    block[..remainder].copy_from_slice(tail);
    let mut k1 = u64::from_le_bytes(block[..8].try_into().expect("eight-byte tail"));
    let mut k2 = u64::from_le_bytes(block[8..].try_into().expect("eight-byte tail"));
    if tail_len > 8 {
        k2 = k2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        h2 ^= k2;
    }
    if tail_len > 0 {
        k1 = k1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        h1 ^= k1;
    }
    h1 ^= effective_len as u64;
    h2 ^= effective_len as u64;
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    h1 = fmix64(h1);
    h2 = fmix64(h2);
    h1 = h1.wrapping_add(h2);
    h2 = h2.wrapping_add(h1);
    format!("{h1:016X}{h2:016X}")
}

fn fmix64(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51_afd7_ed55_8ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    value ^ (value >> 33)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    fn encode_base64(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut output = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            output.push(char::from(TABLE[usize::from(a >> 2)]));
            output.push(char::from(TABLE[usize::from(((a & 3) << 4) | (b >> 4))]));
            output.push(if chunk.len() > 1 {
                char::from(TABLE[usize::from(((b & 15) << 2) | (c >> 6))])
            } else {
                '='
            });
            output.push(if chunk.len() > 2 {
                char::from(TABLE[usize::from(c & 63)])
            } else {
                '='
            });
        }
        output
    }

    fn board_with_embedded(checksum: &str) -> PhysicalBoardDocument {
        board_with_embedded_metadata("", checksum)
    }

    fn board_with_local_metadata(
        local_checksum: &str,
        board_checksum: &str,
    ) -> PhysicalBoardDocument {
        let local_metadata = format!(
            "(embedded_files (file (name \"chip.step\") (type model) (checksum \"{local_checksum}\")))"
        );
        board_with_embedded_metadata(&local_metadata, board_checksum)
    }

    fn board_with_embedded_metadata(local_metadata: &str, checksum: &str) -> PhysicalBoardDocument {
        let compressed = include_bytes!("../tests/fixtures/embedded-models/step-sample.zst");
        let base64 = encode_base64(compressed);
        let payload = base64
            .as_bytes()
            .chunks(76)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let source = format!(
            "(kicad_pcb (version 20250114) (layers (0 F.Cu signal)) (footprint Test:U1 (uuid comp-1) (at 1 2) (property Reference U1) (model kicad-embed://chip.step) {local_metadata}) (embedded_files (file (name \"chip.step\") (type model) (checksum \"{checksum}\") (data |\n{payload}\n|))))"
        );
        let filename = format!(
            "tokito-embedded-{}-{}.kicad_pcb",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(filename);
        std::fs::write(&path, source).unwrap();
        let board = crate::load_board_input(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        board
    }

    #[test]
    fn extracts_and_verifies_embedded_zstandard_asset() {
        let model = b"ISO-10303-21;\nEND-ISO-10303-21;\n";
        let board = board_with_embedded("36B783AF2314FC403FDE27515CB137B3");
        assert_eq!(board.engineering().embedded_files.len(), 1);
        let resolved = resolve_model(&board, &board.engineering().models[0], 1024).unwrap();
        assert_eq!(resolved.name, "chip.step");
        assert_eq!(resolved.bytes, model);
        assert_eq!(
            resolved.checksum_algorithm,
            EmbeddedChecksumAlgorithm::Mmh3X64_128
        );
    }

    #[test]
    fn metadata_only_component_record_falls_back_to_board_payload() {
        let model = b"ISO-10303-21;\nEND-ISO-10303-21;\n";
        let board = board_with_local_metadata(
            "36B783AF2314FC403FDE27515CB137B3",
            "36b783af2314fc403fde27515cb137b3",
        );
        let resolved = resolve_model(&board, &board.engineering().models[0], 1024).unwrap();
        assert_eq!(resolved.bytes, model);
        assert_eq!(resolved.source.component_ordinal, None);
    }

    #[test]
    fn metadata_only_component_record_rejects_board_checksum_conflict() {
        let board = board_with_local_metadata(
            "36B783AF2314FC403FDE27515CB137B3",
            "00000000000000000000000000000000",
        );
        assert_eq!(
            resolve_model(&board, &board.engineering().models[0], 1024).unwrap_err(),
            EmbeddedModelError::MetadataConflict
        );
    }

    #[test]
    fn resolver_reports_embedded_origin_without_a_fake_local_path() {
        use crate::model_resolver::{
            ModelAssetOrigin, ModelResolutionStatus, ModelResolver, ModelResolverConfig,
        };
        let model = b"ISO-10303-21;\nEND-ISO-10303-21;\n";
        let board = board_with_embedded("36B783AF2314FC403FDE27515CB137B3");
        let resolver = ModelResolver::new(ModelResolverConfig::new(std::env::temp_dir())).unwrap();
        let reference = &board.engineering().models[0];
        assert_eq!(
            resolver.resolve(reference).status,
            ModelResolutionStatus::EmbeddedContextRequired
        );
        let resolved = resolver.resolve_board_model(&board, reference);
        assert_eq!(resolved.status, ModelResolutionStatus::Resolved);
        let asset = resolved.asset.unwrap();
        assert!(asset.local_path().is_none());
        assert!(matches!(
            asset.origin(),
            ModelAssetOrigin::EmbeddedBoard { name, .. } if name == "chip.step"
        ));
        assert_eq!(asset.bytes(), model);
    }

    #[test]
    fn rejects_checksum_mismatch_and_trailing_zstandard_data() {
        let board = board_with_embedded("00000000000000000000000000000000");
        assert_eq!(
            resolve_model(&board, &board.engineering().models[0], 1024).unwrap_err(),
            EmbeddedModelError::ChecksumMismatch
        );
        let mut compressed =
            include_bytes!("../tests/fixtures/embedded-models/step-sample.zst").to_vec();
        compressed.extend_from_slice(b"trailing");
        assert_eq!(
            decompress_single_frame(&compressed, 1024),
            Err(EmbeddedModelError::MalformedData)
        );
    }

    #[test]
    fn base64_decoder_rejects_noncanonical_padding_and_size_overflow() {
        assert_eq!(
            decode_base64(b"Zh==", 16),
            Err(EmbeddedModelError::MalformedData)
        );
        assert_eq!(decode_base64(b"Zg==", 0), Err(EmbeddedModelError::TooLarge));
        assert_eq!(decode_base64(b"Zg==", 1).unwrap(), b"f");
    }

    #[test]
    fn mmh3_checksum_matches_independent_resistor_model_vector() {
        let bytes = include_bytes!(
            "../tests/fixtures/open-source/kicad-resistor-models/Resistor_SMD.3dshapes/R_0603_1608Metric.wrl"
        );
        assert_eq!(
            mmh3_x64_128(bytes, 0xABBA_2345, false),
            "F634756E66E171074044BE35B9F489A8"
        );
        assert_eq!(
            mmh3_x64_128(bytes, 0xABBA_2345, true),
            "B53FCF54458A8A78ECD7EC4D28E78CB0"
        );
    }
}
