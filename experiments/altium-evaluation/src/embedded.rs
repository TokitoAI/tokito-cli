//! Bounded complete zlib decoding; no recovered CAD files are written.
use flate2::{Decompress, FlushDecompress, Status};

pub const MAX_MODEL: usize = 32 * 1024 * 1024;

pub fn decode_with_limit(data: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let capacity = limit.checked_add(1).ok_or("model_output_limit_overflow")?;
    let mut output = vec![0; capacity];
    let mut decoder = Decompress::new(true);
    let status = decoder
        .decompress(data, &mut output, FlushDecompress::Finish)
        .map_err(|e| format!("embedded_zlib_error:{e}"))?;
    let count = usize::try_from(decoder.total_out()).map_err(|_| "model_output_count_overflow")?;
    if count > limit {
        return Err("embedded_model_output_limit_exceeded".into());
    }
    if status != Status::StreamEnd {
        return Err("embedded_model_incomplete_zlib_stream".into());
    }
    if decoder.total_in() != data.len() as u64 {
        return Err("embedded_model_trailing_compressed_bytes".into());
    }
    output.truncate(count);
    if !output.starts_with(b"ISO-10303-21;") {
        return Err("embedded_model_not_STEP".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn compressed(data: &[u8]) -> Vec<u8> {
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }
    #[test]
    fn complete_stream_requires_step_and_respects_exact_limit() {
        let data = b"ISO-10303-21; HEADER;";
        assert_eq!(
            decode_with_limit(&compressed(data), data.len()).unwrap(),
            data
        );
        assert!(decode_with_limit(&compressed(b"not CAD"), 100).is_err());
        assert_eq!(
            decode_with_limit(&compressed(data), data.len() - 1).unwrap_err(),
            "embedded_model_output_limit_exceeded"
        );
    }
    #[test]
    fn truncation_corruption_and_trailing_data_refuse() {
        let raw = compressed(b"ISO-10303-21; HEADER;");
        for end in [0, 1, raw.len() - 1] {
            assert!(decode_with_limit(&raw[..end], 100).is_err());
        }
        let mut changed = raw.clone();
        changed[5] ^= 0xff;
        assert!(decode_with_limit(&changed, 100).is_err());
        let mut trailing = raw;
        trailing.extend(b"extra");
        assert_eq!(
            decode_with_limit(&trailing, 100).unwrap_err(),
            "embedded_model_trailing_compressed_bytes"
        );
    }
}
