//! Reject duplicate object keys at every level instead of accepting last-key-wins JSON.
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};
use std::fmt;

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("JSON without duplicate object keys")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<StrictValue, E> {
        Ok(StrictValue(Value::Bool(value)))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<StrictValue, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<StrictValue, E> {
        Ok(StrictValue(Value::Number(Number::from(value))))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<StrictValue, E> {
        Number::from_f64(value)
            .map(|number| StrictValue(Value::Number(number)))
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<StrictValue, E> {
        Ok(StrictValue(Value::String(value.to_owned())))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<StrictValue, E> {
        Ok(StrictValue(Value::String(value)))
    }
    fn visit_unit<E: de::Error>(self) -> Result<StrictValue, E> {
        Ok(StrictValue(Value::Null))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<StrictValue, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(Value::Array(values)))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<StrictValue, A::Error> {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON object key"));
            }
            let value = object.next_value::<StrictValue>()?;
            values.insert(key, value.0);
        }
        Ok(StrictValue(Value::Object(values)))
    }
}

pub(super) fn parse(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str::<StrictValue>(text).map(|value| value.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_nested_keys_and_trailing_data() {
        assert!(parse(r#"{"source":{"path":"bad","path":"good"}}"#).is_err());
        assert!(parse(r#"{"a":1} {}"#).is_err());
        assert_eq!(
            parse(r#"{"a":[1,-2,0.5,true,null,"text"]}"#).expect("JSON"),
            serde_json::json!({"a":[1,-2,0.5,true,null,"text"]})
        );
    }
}
