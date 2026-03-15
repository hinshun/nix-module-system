//! Value conversion between Nix and our internal types.
//!
//! This module handles converting nix-bindings-rust values to our `Value` type
//! and vice versa.

use crate::types::Value;

#[cfg(feature = "nix-bindings")]
use crate::nix::error::{NixError, NixResult};
#[cfg(feature = "nix-bindings")]
use indexmap::IndexMap;
#[cfg(feature = "nix-bindings")]
use std::path::PathBuf;

#[cfg(feature = "nix-bindings")]
use nix_bindings_expr::eval_state::EvalState;
#[cfg(feature = "nix-bindings")]
use nix_bindings_expr::value::Value as NixValue;

/// Convert a Nix value to our internal Value type.
///
/// This recursively converts Nix values, handling:
/// - Primitives (null, bool, int, float, string)
/// - Paths
/// - Lists
/// - Attribute sets
/// - Lambdas (converted to Lambda variant)
/// - Derivations (detected via `type = "derivation"`)
#[cfg(feature = "nix-bindings")]
pub fn nix_to_value(state: &mut EvalState, nix_val: &NixValue) -> NixResult<Value> {
    use nix_bindings_expr::value::ValueType;

    // Get the type of the value
    let val_type = state.value_type(nix_val);

    match val_type {
        ValueType::Null => Ok(Value::Null),

        ValueType::Bool => {
            let b = state
                .require_bool(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Bool(b))
        }

        ValueType::Int => {
            let i = state
                .require_int(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Int(i))
        }

        ValueType::Float => {
            let f = state
                .require_float(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Float(f))
        }

        ValueType::String => {
            let s = state
                .require_string(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::String(s))
        }

        ValueType::Path => {
            let p = state
                .require_path(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Path(PathBuf::from(p)))
        }

        ValueType::List => {
            let size = state
                .require_list_size(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;

            let mut elements = Vec::with_capacity(size as usize);
            for i in 0..size {
                if let Some(elem) = state
                    .require_list_select_idx_strict(nix_val, i)
                    .map_err(|e| NixError::evaluation(e.to_string()))?
                {
                    elements.push(nix_to_value(state, &elem)?);
                }
            }
            Ok(Value::List(elements))
        }

        ValueType::Attrs => {
            // Check if this is a derivation
            if let Ok(Some(type_val)) = state.require_attrs_select_opt(nix_val, "type") {
                if let Ok(type_str) = state.require_string(&type_val) {
                    if type_str == "derivation" {
                        // Convert the derivation attrs
                        let inner = nix_attrs_to_value(state, nix_val)?;
                        return Ok(Value::Derivation(Box::new(inner)));
                    }
                }
            }

            nix_attrs_to_value(state, nix_val)
        }

        ValueType::Lambda | ValueType::PrimOp | ValueType::PrimOpApp => Ok(Value::Lambda),

        ValueType::Thunk => {
            // Force the thunk and convert
            state
                .force(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            nix_to_value(state, nix_val)
        }

        ValueType::External => {
            // External values can't be converted
            Err(NixError::conversion("cannot convert external value"))
        }
    }
}

/// Convert Nix attrs to our Value::Attrs.
#[cfg(feature = "nix-bindings")]
fn nix_attrs_to_value(state: &mut EvalState, nix_val: &NixValue) -> NixResult<Value> {
    let names = state
        .require_attrs_names(nix_val)
        .map_err(|e| NixError::evaluation(e.to_string()))?;

    let mut attrs = IndexMap::with_capacity(names.len());
    for name in names {
        let attr_val = state
            .require_attrs_select(nix_val, &name)
            .map_err(|e| NixError::evaluation(e.to_string()))?;
        attrs.insert(name, nix_to_value(state, &attr_val)?);
    }

    Ok(Value::Attrs(attrs))
}

/// Convert our Value type to a Nix value.
///
/// This creates Nix values from our internal representation.
#[cfg(feature = "nix-bindings")]
pub fn value_to_nix(state: &mut EvalState, value: &Value) -> NixResult<NixValue> {
    match value {
        Value::Null => state
            .new_value_null()
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Bool(b) => state
            .new_value_bool(*b)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Int(i) => state
            .new_value_int(*i)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Float(f) => state
            .new_value_float(*f)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::String(s) => state
            .new_value_str(s)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Path(p) => state
            .new_value_path(&p.to_string_lossy())
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::List(items) => {
            let mut nix_items = Vec::with_capacity(items.len());
            for item in items {
                nix_items.push(value_to_nix(state, item)?);
            }
            state
                .new_value_list(nix_items)
                .map_err(|e| NixError::evaluation(e.to_string()))
        }

        Value::Attrs(attrs) => {
            let mut nix_attrs = Vec::with_capacity(attrs.len());
            for (k, v) in attrs {
                nix_attrs.push((k.clone(), value_to_nix(state, v)?));
            }
            state
                .new_value_attrs(nix_attrs)
                .map_err(|e| NixError::evaluation(e.to_string()))
        }

        Value::Lambda => {
            // Cannot convert lambdas back to Nix
            Err(NixError::conversion(
                "cannot convert lambda to Nix value without context",
            ))
        }

        Value::Derivation(inner) => {
            // Convert the inner value and it will have type = "derivation"
            value_to_nix(state, inner)
        }
    }
}

/// Lazy value wrapper that defers conversion until needed.
///
/// This allows working with Nix values without forcing full evaluation.
#[cfg(feature = "nix-bindings")]
pub struct LazyValue {
    /// The underlying Nix value.
    nix_value: NixValue,
    /// Cached converted value.
    cached: Option<Value>,
}

#[cfg(feature = "nix-bindings")]
impl LazyValue {
    /// Create a new lazy value.
    pub fn new(nix_value: NixValue) -> Self {
        Self {
            nix_value,
            cached: None,
        }
    }

    /// Get the underlying Nix value.
    pub fn nix_value(&self) -> &NixValue {
        &self.nix_value
    }

    /// Force evaluation and convert to our Value type.
    pub fn force(&mut self, state: &mut EvalState) -> NixResult<&Value> {
        if self.cached.is_none() {
            let value = nix_to_value(state, &self.nix_value)?;
            self.cached = Some(value);
        }
        Ok(self.cached.as_ref().unwrap())
    }

    /// Take the converted value, consuming self.
    pub fn into_value(mut self, state: &mut EvalState) -> NixResult<Value> {
        self.force(state)?;
        Ok(self.cached.unwrap())
    }
}

// Non-feature-gated helpers for testing without nix-bindings

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
