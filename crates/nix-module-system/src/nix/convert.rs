//! Value conversion helpers.
//!
//! This module provides conversion between JSON and our internal `Value` type.

use crate::types::Value;

/// Convert a JSON value to our Value type.
///
/// This is useful for testing and for loading configuration from files.
pub fn json_to_value(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(arr) => {
            Value::List(arr.iter().map(json_to_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let attrs = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::Attrs(attrs)
        }
    }
}

/// Convert our Value type to JSON.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::Value::Number((*i).into()),
        Value::Float(f) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Path(p) => serde_json::Value::String(p.to_string_lossy().to_string()),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Attrs(attrs) => {
            let obj: serde_json::Map<String, serde_json::Value> = attrs
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Lambda => serde_json::Value::String("<lambda>".to_string()),
        Value::Derivation(inner) => value_to_json(inner),
    }
}

#[cfg(test)]
mod convert_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_json_roundtrip() {
        let json = serde_json::json!({
            "name": "test",
            "enabled": true,
            "count": 42,
            "ratio": 3.14,
            "items": [1, 2, 3],
            "nested": {
                "foo": "bar"
            }
        });

        let value = json_to_value(&json);
        let back = value_to_json(&value);

        assert_eq!(json, back);
    }

    #[test]
    fn test_json_to_value_primitives() {
        assert_eq!(json_to_value(&serde_json::Value::Null), Value::Null);
        assert_eq!(
            json_to_value(&serde_json::json!(true)),
            Value::Bool(true)
        );
        assert_eq!(json_to_value(&serde_json::json!(42)), Value::Int(42));
        assert_eq!(
            json_to_value(&serde_json::json!(3.14)),
            Value::Float(3.14)
        );
        assert_eq!(
            json_to_value(&serde_json::json!("hello")),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_value_to_json_path() {
        let value = Value::Path(PathBuf::from("/nix/store/abc123"));
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::json!("/nix/store/abc123"));
    }

    #[test]
    fn test_value_to_json_lambda() {
        let value = Value::Lambda;
        let json = value_to_json(&value);
        assert_eq!(json, serde_json::json!("<lambda>"));
    }
}
