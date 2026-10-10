//! Original bounded framing inspection for the observed dataset stream profile.
use serde_json::{json, Value};

fn u32_at(data: &[u8], at: usize) -> Result<usize, String> {
    let b = data.get(at..at + 4).ok_or("truncated_record_length")?;
    Ok(u32::from_le_bytes(b.try_into().unwrap()) as usize)
}
fn fields(data: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(data.strip_suffix(&[0]).unwrap_or(data))
        .map_err(|_| "non_utf8_property_record")?;
    if text.contains('\0') {
        return Err("embedded_property_nul".into());
    }
    let mut fields = serde_json::Map::new();
    for field in text.split('|').filter(|f| !f.is_empty()) {
        let (key, value) = field.split_once('=').ok_or("property_missing_equals")?;
        if key.is_empty() || key.len() > 128 {
            return Err("invalid_property_key".into());
        }
        if let Some(old) = fields.get(key) {
            if old != value {
                return Err(format!("conflicting_property:{key}"));
            }
        }
        fields.insert(key.into(), json!(value));
    }
    Ok(Value::Object(fields))
}

/// Preserve legacy non-ASCII values without guessing a source code page.
fn ascii_fields(data: &[u8]) -> Result<Value, String> {
    let data = data.strip_suffix(&[0]).unwrap_or(data);
    if data.contains(&0) {
        return Err("embedded_property_nul".into());
    }
    let mut fields = serde_json::Map::new();
    for field in data.split(|b| *b == b'|').filter(|f| !f.is_empty()) {
        let split = field
            .iter()
            .position(|b| *b == b'=')
            .ok_or("property_missing_equals")?;
        let (key, tail) = field.split_at(split);
        if key.is_empty() || key.len() > 128 || !key.is_ascii() {
            return Err("invalid_ascii_property_key".into());
        }
        let key = std::str::from_utf8(key).map_err(|_| "invalid_ascii_property_key")?;
        let bytes = &tail[1..];
        let value = if bytes.is_ascii() {
            json!(std::str::from_utf8(bytes).map_err(|_| "invalid_ascii_property_value")?)
        } else {
            json!({"encodingResolved":false,"rawByteHex":bytes.iter().map(|b|format!("{b:02x}")).collect::<String>()})
        };
        if let Some(old) = fields.get_mut(key) {
            if let Some(values) = old.get_mut("ambiguousValues").and_then(Value::as_array_mut) {
                values.push(value);
            } else if old != &value {
                *old = json!({"ambiguousValues":[old.clone(),value],"resolved":false});
            }
        } else {
            fields.insert(key.into(), value);
        }
    }
    Ok(Value::Object(fields))
}

pub fn inspect_ascii(data: &[u8]) -> Result<Vec<Value>, String> {
    inspect_profile(data, false, true)
}

pub fn inspect(data: &[u8], bodies: bool) -> Result<Vec<Value>, String> {
    inspect_profile(data, bodies, false)
}

fn inspect_profile(data: &[u8], bodies: bool, ascii_only: bool) -> Result<Vec<Value>, String> {
    let mut result = Vec::new();
    let mut cursor = 0usize;
    while cursor < data.len() {
        if result.len() >= 2048 {
            return Err("record_count_limit_exceeded".into());
        }
        let prefix = if bodies { 5 } else { 4 };
        if bodies && data[cursor] != 12 {
            return Err("unsupported_body_record_tag".into());
        }
        let length = u32_at(data, cursor + usize::from(bodies))?;
        let start = cursor.checked_add(prefix).ok_or("record_offset_overflow")?;
        let end = start.checked_add(length).ok_or("record_size_overflow")?;
        let payload = data.get(start..end).ok_or("truncated_record_payload")?;
        let mut record = json!({"ordinal":result.len(),"byteOffset":cursor,"payloadBytes":length});
        if bodies {
            // Observed profile: 18-byte fixed prefix, then a length-framed
            // property block. All remaining body bytes stay uninterpreted.
            let property_length = u32_at(payload, 18)?;
            let properties = payload
                .get(
                    22..22usize
                        .checked_add(property_length)
                        .ok_or("property_size_overflow")?,
                )
                .ok_or_else(|| format!("truncated_body_properties:record={},offset={cursor},payload={},properties={property_length},prefix={}",result.len(),payload.len(),payload.iter().take(32).map(|b|format!("{b:02x}")).collect::<String>()))?;
            let candidate = payload
                .get(7..9)
                .ok_or("truncated_body_component_candidate")?;
            record["componentIndexCandidate"] =
                json!(u16::from_le_bytes(candidate.try_into().unwrap()));
            record["componentIndexSemanticsIndependentlyVerified"] = json!(false);
            record["properties"] = fields(properties)?;
            record["remainingPayloadBytes"] = json!(payload.len() - 22 - property_length);
            record["remainingPayloadDecoded"] = json!(false);
        } else {
            if ascii_only {
                let bytes = payload.strip_suffix(&[0]).unwrap_or(payload);
                let mut sections = Vec::new();
                let mut offset = 0;
                for section in bytes.split(|b| *b == b'\r') {
                    if sections.len() == 2048 {
                        return Err("property_section_count_limit_exceeded".into());
                    }
                    if !section.trim_ascii().is_empty() {
                        sections.push(json!({"sectionOrdinal":sections.len(),"payloadByteOffset":offset,
                            "rawSectionBytes":section.len(),"properties":ascii_fields(section.trim_ascii())?}));
                    }
                    offset += section.len() + 1;
                }
                record["properties"] = sections
                    .first()
                    .map(|s| s["properties"].clone())
                    .unwrap_or(json!({}));
                record["propertySections"] = json!(sections);
                record["propertyEncoding"] = json!(
                    "ASCII values; non-ASCII bytes preserved without code-page interpretation"
                );
                record["sectionScope"] = json!("properties contains first CR-separated section only; subsequent sections are not merged");
            } else {
                record["properties"] = fields(payload)?;
            }
        }
        result.push(record);
        cursor = end;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn length_framing_requires_complete_payload_and_preserves_ordinals() {
        let mut bytes = Vec::new();
        for text in [b"|ID=A\0".as_slice(), b"|ID=B\0".as_slice()] {
            bytes.extend((text.len() as u32).to_le_bytes());
            bytes.extend(text);
        }
        let r = inspect(&bytes, false).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1]["properties"]["ID"], "B");
        bytes.pop();
        assert!(inspect(&bytes, false).is_err());
    }
    #[test]
    fn conflicting_duplicates_refuse_and_equal_duplicates_preserve_value() {
        assert!(fields(b"|A=1|A=2\0").is_err());
        assert_eq!(fields(b"|A=1|A=1\0").unwrap()["A"], "1");
    }
    #[test]
    fn body_unknown_tags_and_truncated_lengths_refuse() {
        assert!(inspect(&[11, 0, 0, 0, 0], true).is_err());
        assert!(inspect(&[12, 1, 0], true).is_err());
        assert!(inspect(&[12, 0, 0, 0, 0], true).is_err());
    }
    #[test]
    fn legacy_metadata_preserves_bytes_without_assuming_utf8() {
        let payload = b"|THICKNESS=15.4mil|NAME=\xff\xe9\0";
        let mut data = (payload.len() as u32).to_le_bytes().to_vec();
        data.extend(payload);
        assert!(inspect(&data, false).is_err());
        let report = inspect_ascii(&data).unwrap();
        assert_eq!(report[0]["properties"]["THICKNESS"], "15.4mil");
        assert_eq!(report[0]["properties"]["NAME"]["rawByteHex"], "ffe9");
        assert_eq!(report[0]["properties"]["NAME"]["encodingResolved"], false);
        assert_eq!(
            ascii_fields(b"|X=1|X=2|X=3\0").unwrap()["X"]["ambiguousValues"],
            json!(["1", "2", "3"])
        );
        assert!(ascii_fields(b"|\xff=1\0").is_err());
        assert!(ascii_fields(b"|X=1\0|Y=2").is_err());
        let sections = b"|RECORD=Board|X=1\r|RECORD=Board|X=2\0";
        let mut framed = (sections.len() as u32).to_le_bytes().to_vec();
        framed.extend(sections);
        let parsed = inspect_ascii(&framed).unwrap();
        assert_eq!(parsed[0]["properties"]["X"], "1");
        assert_eq!(parsed[0]["propertySections"][1]["properties"]["X"], "2");
        data.pop();
        assert!(inspect_ascii(&data).is_err());
    }
}
