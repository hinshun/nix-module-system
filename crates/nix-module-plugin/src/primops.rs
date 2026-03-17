//! Primop callback implementations.
//!
//! Each function here is a Nix primop callback registered in `nix_plugin_entry`.
//!
//! **Important**: The Nix C API `EvalState*` passed to primop callbacks is actually
//! a raw `nix::EvalState*`, NOT the C API wrapper struct. C API functions that
//! take `EvalState*` will segfault with this pointer.
//!
//! **Workaround**: All complex arguments (type descriptors, values, definitions)
//! are pre-serialized to JSON on the Nix side using `builtins.toJSON`. The primops
//! receive JSON strings, which can be read with the string getter (no EvalState
//! needed). Results are returned as JSON strings, wrapped with `builtins.fromJSON`
//! on the Nix side.

use crate::ffi;
use crate::type_resolve;

use nix_module_system::merge::{process_conditional, MergeEngine};
use nix_module_system::nix::{json_to_value, value_to_json};
use nix_module_system::types::{Definition, OptionPath, Value};

use std::ffi::{c_void, CString};

// ---------------------------------------------------------------------------
// __nms_version (arity 0)
// ---------------------------------------------------------------------------

/// Returns the plugin version string.
///
/// # Safety
///
/// Called by the Nix evaluator with valid pointers.
pub unsafe extern "C" fn primop_version(
    _user_data: *mut c_void,
    ctx: *mut ffi::nix_c_context,
    _state: *mut ffi::EvalState,
    _args: *mut *mut ffi::nix_value,
    result: *mut ffi::nix_value,
) {
    let version = concat!(env!("CARGO_PKG_VERSION"), "\0");
    let cs = version.as_ptr() as *const std::ffi::c_char;
    ffi::nix_init_string(ctx, result, cs);
}

// ---------------------------------------------------------------------------
// __nms_checkType (arity 2): typeDescJSON valueJSON -> bool
// ---------------------------------------------------------------------------

/// Type-check a value against a type descriptor.
///
/// Args:
///   0: typeDescJSON — JSON string encoding the type descriptor
///   1: valueJSON — JSON string encoding the value to check
///
/// Returns: `true` if the value matches the type, `false` otherwise.
///
/// # Safety
///
/// Called by the Nix evaluator with valid pointers.
pub unsafe extern "C" fn primop_check_type(
    _user_data: *mut c_void,
    ctx: *mut ffi::nix_c_context,
    state: *mut ffi::EvalState,
    args: *mut *mut ffi::nix_value,
    result: *mut ffi::nix_value,
) {
    let ok = (|| -> Result<bool, String> {
        let type_desc_nix = *args.add(0);
        let value_nix = *args.add(1);

        // Force thunks (args are always lazy)
        ffi::nix_force_value_deep(state, type_desc_nix);
        ffi::nix_force_value_deep(state, value_nix);

        // Read JSON strings
        let type_desc_json = ffi::get_string(ctx, type_desc_nix)?;
        let value_json = ffi::get_string(ctx, value_nix)?;

        // Parse JSON to Values
        let type_desc_parsed: serde_json::Value = serde_json::from_str(&type_desc_json)
            .map_err(|e| format!("invalid type descriptor JSON: {}", e))?;
        let value_parsed: serde_json::Value = serde_json::from_str(&value_json)
            .map_err(|e| format!("invalid value JSON: {}", e))?;

        let type_desc = json_to_value(&type_desc_parsed);
        let value = json_to_value(&value_parsed);

        let nix_type = type_resolve::resolve_type(&type_desc)?;
        Ok(nix_type.check(&value).is_ok())
    })();

    match ok {
        Ok(b) => {
            ffi::nix_init_bool(ctx, result, b);
        }
        Err(e) => {
            eprintln!("__nms_checkType error: {}", e);
            ffi::nix_init_bool(ctx, result, false);
        }
    }
}

// ---------------------------------------------------------------------------
// __nms_processConditionals (arity 1): valueJSON -> defsJSON
// ---------------------------------------------------------------------------

/// Flatten mkIf/mkMerge/mkOverride wrappers into a list of active definitions.
///
/// Args:
///   0: valueJSON — JSON string encoding a value with mkIf/mkMerge/mkOverride
///
/// Returns: JSON string encoding `[{ "value": ..., "priority": N }, ...]`.
///   The Nix side wraps with `builtins.fromJSON`.
///
/// # Safety
///
/// Called by the Nix evaluator with valid pointers.
pub unsafe extern "C" fn primop_process_conditionals(
    _user_data: *mut c_void,
    ctx: *mut ffi::nix_c_context,
    state: *mut ffi::EvalState,
    args: *mut *mut ffi::nix_value,
    result: *mut ffi::nix_value,
) {
    let res = (|| -> Result<String, String> {
        let value_nix = *args.add(0);

        // Force thunk
        ffi::nix_force_value_deep(state, value_nix);

        // Read JSON string
        let value_json = ffi::get_string(ctx, value_nix)?;

        // Parse JSON to Value
        let parsed: serde_json::Value = serde_json::from_str(&value_json)
            .map_err(|e| format!("invalid value JSON: {}", e))?;
        let value = json_to_value(&parsed);

        // Process override wrappers to extract priority
        let (inner, priority) = extract_override(value);

        // Process conditionals (mkIf, mkMerge)
        let active = process_conditional(inner);

        // Build result as JSON
        let result_items: Vec<serde_json::Value> = active
            .into_iter()
            .map(|v| {
                let (inner_v, inner_p) = extract_override(v);
                let p = if inner_p != 100 { inner_p } else { priority };
                serde_json::json!({
                    "value": value_to_json(&inner_v),
                    "priority": p,
                })
            })
            .collect();

        serde_json::to_string(&result_items)
            .map_err(|e| format!("JSON serialization failed: {}", e))
    })();

    match res {
        Ok(json) => set_result_string(ctx, result, &json),
        Err(e) => {
            eprintln!("__nms_processConditionals error: {}", e);
            ffi::nix_init_null(ctx, result);
        }
    }
}

// ---------------------------------------------------------------------------
// __nms_mergeDefinitions (arity 3): name typeDescJSON defsJSON -> resultJSON
// ---------------------------------------------------------------------------

/// Core merge engine — merges multiple definitions for an option.
///
/// Args:
///   0: name — string, the option path (for error messages)
///   1: typeDescJSON — JSON string encoding the type descriptor
///   2: defsJSON — JSON string encoding the definitions list
///
/// Returns: JSON string encoding the merged value.
///   The Nix side wraps with `builtins.fromJSON`.
///
/// # Safety
///
/// Called by the Nix evaluator with valid pointers.
pub unsafe extern "C" fn primop_merge_definitions(
    _user_data: *mut c_void,
    ctx: *mut ffi::nix_c_context,
    state: *mut ffi::EvalState,
    args: *mut *mut ffi::nix_value,
    result: *mut ffi::nix_value,
) {
    let res = (|| -> Result<String, String> {
        let name_nix = *args.add(0);
        let type_desc_nix = *args.add(1);
        let defs_nix = *args.add(2);

        // Force all args
        ffi::nix_force_value_deep(state, name_nix);
        ffi::nix_force_value_deep(state, type_desc_nix);
        ffi::nix_force_value_deep(state, defs_nix);

        // Read strings
        let name = ffi::get_string(ctx, name_nix)
            .map_err(|_| "first argument (name) must be a string".to_string())?;
        let type_desc_json = ffi::get_string(ctx, type_desc_nix)
            .map_err(|_| "second argument (typeDesc) must be a JSON string".to_string())?;
        let defs_json = ffi::get_string(ctx, defs_nix)
            .map_err(|_| "third argument (defs) must be a JSON string".to_string())?;

        let path = OptionPath::from_dotted(&name);

        // Parse type descriptor
        let type_desc_parsed: serde_json::Value = serde_json::from_str(&type_desc_json)
            .map_err(|e| format!("invalid type descriptor JSON: {}", e))?;
        let type_desc = json_to_value(&type_desc_parsed);
        let nix_type = type_resolve::resolve_type(&type_desc)?;

        // Parse definitions
        let defs_parsed: serde_json::Value = serde_json::from_str(&defs_json)
            .map_err(|e| format!("invalid definitions JSON: {}", e))?;
        let defs_value = json_to_value(&defs_parsed);

        let defs_list = match defs_value {
            Value::List(l) => l,
            _ => return Err("definitions must be a list".to_string()),
        };

        // Convert to Definition structs
        let definitions: Vec<Definition> = defs_list
            .into_iter()
            .map(|item| match item {
                Value::Attrs(ref attrs) => {
                    let value = attrs
                        .get("value")
                        .cloned()
                        .unwrap_or_else(|| item.clone());
                    let priority = match attrs.get("priority") {
                        Some(Value::Int(p)) => *p as i32,
                        _ => 100,
                    };
                    Definition::with_priority(value, priority)
                }
                other => Definition::new(other),
            })
            .collect();

        if definitions.is_empty() {
            return Err(format!("no definitions for option '{}'", name));
        }

        // Run the merge engine
        let mut engine = MergeEngine::new();
        let merge_result = engine
            .merge_option(nix_type.as_ref(), &path, definitions)
            .map_err(|e| format!("merge error for '{}': {}", name, e))?;

        let json = value_to_json(&merge_result.value);
        serde_json::to_string(&json)
            .map_err(|e| format!("JSON serialization failed: {}", e))
    })();

    match res {
        Ok(json) => set_result_string(ctx, result, &json),
        Err(e) => {
            eprintln!("__nms_mergeDefinitions error: {}", e);
            ffi::nix_init_null(ctx, result);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set the primop result to a string value. Does NOT use EvalState.
///
/// # Safety
///
/// `ctx` and `result` must be valid pointers.
unsafe fn set_result_string(
    ctx: *mut ffi::nix_c_context,
    result: *mut ffi::nix_value,
    s: &str,
) {
    match CString::new(s) {
        Ok(cs) => {
            ffi::nix_init_string(ctx, result, cs.as_ptr());
        }
        Err(_) => {
            eprintln!("set_result_string: string contains null byte");
            ffi::nix_init_null(ctx, result);
        }
    }
}

/// Extract mkOverride wrapper, returning (inner_value, priority).
/// Default priority is 100 if no override wrapper is present.
fn extract_override(value: Value) -> (Value, i32) {
    match &value {
        Value::Attrs(attrs) => {
            if matches!(attrs.get("_type"), Some(Value::String(s)) if s == "override") {
                let priority = match attrs.get("priority") {
                    Some(Value::Int(p)) => *p as i32,
                    _ => 100,
                };
                let content = attrs.get("content").cloned().unwrap_or(Value::Null);
                // Recurse in case of nested overrides
                let (inner, inner_p) = extract_override(content);
                // Inner override takes precedence (lower number = higher priority)
                let final_p = if inner_p != 100 { inner_p } else { priority };
                (inner, final_p)
            } else {
                (value, 100)
            }
        }
        _ => (value, 100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_extract_override_none() {
        let val = Value::String("hello".to_string());
        let (inner, priority) = extract_override(val.clone());
        assert_eq!(inner, val);
        assert_eq!(priority, 100);
    }

    #[test]
    fn test_extract_override_mk_default() {
        let mut attrs = IndexMap::new();
        attrs.insert("_type".to_string(), Value::String("override".to_string()));
        attrs.insert("priority".to_string(), Value::Int(1000));
        attrs.insert(
            "content".to_string(),
            Value::String("default_val".to_string()),
        );
        let val = Value::Attrs(attrs);

        let (inner, priority) = extract_override(val);
        assert_eq!(inner, Value::String("default_val".to_string()));
        assert_eq!(priority, 1000);
    }

    #[test]
    fn test_extract_override_mk_force() {
        let mut attrs = IndexMap::new();
        attrs.insert("_type".to_string(), Value::String("override".to_string()));
        attrs.insert("priority".to_string(), Value::Int(50));
        attrs.insert(
            "content".to_string(),
            Value::String("forced_val".to_string()),
        );
        let val = Value::Attrs(attrs);

        let (inner, priority) = extract_override(val);
        assert_eq!(inner, Value::String("forced_val".to_string()));
        assert_eq!(priority, 50);
    }
}
