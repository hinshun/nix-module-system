//! Resolve Nix type descriptor attrsets into Rust `NixType` implementations.
//!
//! Type descriptors are plain Nix attrsets like:
//! ```nix
//! { _type = "type"; name = "str"; }
//! { _type = "type"; name = "listOf"; elemType = { _type = "type"; name = "str"; }; }
//! ```
//!
//! This module pattern-matches on the `name` field to select the corresponding
//! Rust `NixType` implementation.

use nix_module_system::types::{
    AttrsOf, Bool, BoolByOr, Enum, Float, Int, ListOf, NixType, NullOr, OneOf, Path, Str, Value,
};

/// Resolve a type descriptor `Value` (an attrset) into a boxed `NixType`.
///
/// The descriptor must have `_type = "type"` and a `name` field.
/// Compound types have additional fields like `elemType`.
pub fn resolve_type(desc: &Value) -> Result<Box<dyn NixType>, String> {
    let attrs = match desc {
        Value::Attrs(attrs) => attrs,
        _ => return Err(format!("type descriptor must be an attrset, got: {}", desc)),
    };

    // Verify _type = "type"
    match attrs.get("_type") {
        Some(Value::String(t)) if t == "type" => {}
        _ => {
            return Err(
                "type descriptor must have _type = \"type\"".to_string(),
            )
        }
    }

    let name = match attrs.get("name") {
        Some(Value::String(n)) => n.as_str(),
        _ => return Err("type descriptor must have a string 'name' field".to_string()),
    };

    match name {
        // Primitive types
        "bool" => Ok(Box::new(Bool)),
        "boolByOr" => Ok(Box::new(BoolByOr)),
        "int" => Ok(Box::new(Int)),
        "float" => Ok(Box::new(Float)),
        "str" | "lines" => Ok(Box::new(Str)),
        "path" => Ok(Box::new(Path)),
        "package" => Ok(Box::new(Str)), // packages are opaque at this level

        // Port is an int with range check — use Int for now
        "port" => Ok(Box::new(Int)),

        // Compound types
        n if n.starts_with("listOf") => {
            let elem_type = get_elem_type(attrs, "elemType")?;
            Ok(Box::new(ListOf::new(elem_type)))
        }

        n if n.starts_with("attrsOf") => {
            let elem_type = get_elem_type(attrs, "elemType")?;
            Ok(Box::new(AttrsOf::new(elem_type)))
        }

        n if n.starts_with("lazyAttrsOf") => {
            let elem_type = get_elem_type(attrs, "elemType")?;
            Ok(Box::new(AttrsOf::lazy(elem_type)))
        }

        n if n.starts_with("nullOr") => {
            let inner = get_elem_type(attrs, "elemType")
                .or_else(|_| get_elem_type(attrs, "inner"))?;
            Ok(Box::new(NullOr::new(inner)))
        }

        n if n.starts_with("either") => {
            let t1 = get_elem_type(attrs, "t1")?;
            let t2 = get_elem_type(attrs, "t2")?;
            Ok(Box::new(OneOf::new(vec![t1, t2])))
        }

        n if n.starts_with("enum") => {
            let values = match attrs.get("values") {
                Some(Value::List(vs)) => vs
                    .iter()
                    .filter_map(|v| match v {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => return Err("enum type must have a 'values' list".to_string()),
            };
            Ok(Box::new(Enum::new(values)))
        }

        "submodule" => {
            // Submodules are handled by the Nix orchestrator, not by Rust primops.
            // Return a generic attrsOf-any placeholder for merge purposes.
            Ok(Box::new(AttrsOf::new(Box::new(Str))))
        }

        _ => Err(format!("unknown type name: '{}'", name)),
    }
}

/// Extract a nested type descriptor from an attrset field and resolve it.
fn get_elem_type(
    attrs: &indexmap::IndexMap<String, Value>,
    field: &str,
) -> Result<Box<dyn NixType>, String> {
    match attrs.get(field) {
        Some(desc) => resolve_type(desc),
        None => Err(format!("compound type missing '{}' field", field)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn type_desc(name: &str) -> Value {
        let mut attrs = IndexMap::new();
        attrs.insert("_type".to_string(), Value::String("type".to_string()));
        attrs.insert("name".to_string(), Value::String(name.to_string()));
        Value::Attrs(attrs)
    }

    fn compound_type_desc(name: &str, elem_field: &str, elem_name: &str) -> Value {
        let mut attrs = IndexMap::new();
        attrs.insert("_type".to_string(), Value::String("type".to_string()));
        attrs.insert("name".to_string(), Value::String(name.to_string()));
        attrs.insert(elem_field.to_string(), type_desc(elem_name));
        Value::Attrs(attrs)
    }

    #[test]
    fn test_resolve_primitives() {
        assert_eq!(resolve_type(&type_desc("bool")).unwrap().name(), "bool");
        assert_eq!(resolve_type(&type_desc("int")).unwrap().name(), "int");
        assert_eq!(resolve_type(&type_desc("str")).unwrap().name(), "str");
        assert_eq!(resolve_type(&type_desc("float")).unwrap().name(), "float");
        assert_eq!(resolve_type(&type_desc("path")).unwrap().name(), "path");
    }

    #[test]
    fn test_resolve_list_of() {
        let desc = compound_type_desc("listOf str", "elemType", "str");
        let ty = resolve_type(&desc).unwrap();
        assert_eq!(ty.name(), "listOf");
    }

    #[test]
    fn test_resolve_attrs_of() {
        let desc = compound_type_desc("attrsOf str", "elemType", "str");
        let ty = resolve_type(&desc).unwrap();
        assert_eq!(ty.name(), "attrsOf");
    }

    #[test]
    fn test_resolve_null_or() {
        let desc = compound_type_desc("nullOr str", "elemType", "str");
        let ty = resolve_type(&desc).unwrap();
        assert_eq!(ty.name(), "nullOr");
    }

    #[test]
    fn test_resolve_enum() {
        let mut attrs = IndexMap::new();
        attrs.insert("_type".to_string(), Value::String("type".to_string()));
        attrs.insert(
            "name".to_string(),
            Value::String("enum [\"a\" \"b\"]".to_string()),
        );
        attrs.insert(
            "values".to_string(),
            Value::List(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
            ]),
        );
        let ty = resolve_type(&Value::Attrs(attrs)).unwrap();
        assert_eq!(ty.name(), "enum");
    }

    #[test]
    fn test_invalid_descriptor() {
        assert!(resolve_type(&Value::String("not an attrset".to_string())).is_err());
        assert!(resolve_type(&type_desc("nonexistent_type_xyz")).is_err());
    }
}
