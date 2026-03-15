//! Compound types: listOf, attrsOf, nullOr, oneOf, enum

use super::{
    ComposableType, Definition, MergeResult, NixType, OptionPath, PriorityMerge, Value,
};
use crate::errors::TypeError;
use indexmap::IndexMap;
use std::collections::HashMap;

/// List type with element type checking
#[derive(Debug)]
pub struct ListOf {
    elem_type: Box<dyn NixType>,
}

impl ListOf {
    /// Create a new listOf type
    pub fn new(elem_type: Box<dyn NixType>) -> Self {
        Self { elem_type }
    }
}

impl Clone for ListOf {
    fn clone(&self) -> Self {
        Self {
            elem_type: self.elem_type.clone_box(),
        }
    }
}

impl NixType for ListOf {
    fn name(&self) -> &str {
        "listOf"
    }

    fn description(&self) -> String {
        format!("list of {}", self.elem_type.description())
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::List(_) => Ok(()), // Shallow check only
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: format!("{:?}", value),
                value: Some(value.clone()),
            }),
        }
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        let defs = self.filter_by_priority(defs);

        if defs.is_empty() {
            return Ok(MergeResult::from_default(Value::List(vec![])));
        }

        // Concatenate all lists
        let mut result = Vec::new();

        for (def_idx, def) in defs.iter().enumerate() {
            match &def.value {
                Value::List(items) => {
                    for (item_idx, item) in items.iter().enumerate() {
                        // Type check each element
                        let elem_loc = loc.child(&format!("[{}:{}]", def_idx, item_idx));
                        self.elem_type.check(item).map_err(|e| e.at_path(elem_loc))?;
                        result.push(item.clone());
                    }
                }
                _ => {
                    return Err(TypeError::Mismatch {
                        expected: self.description(),
                        found: format!("{:?}", def.value),
                        value: Some(def.value.clone()),
                    })
                }
            }
        }

        Ok(MergeResult::new(Value::List(result)))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::List(vec![]))
    }

    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>> {
        let mut map = HashMap::new();
        map.insert("elemType".to_string(), self.elem_type.clone_box());
        map
    }

    fn can_unify(&self, a: &Value, b: &Value) -> bool {
        matches!((a, b), (Value::List(_), Value::List(_)))
    }

    fn unify(&self, a: &Value, b: &Value) -> Option<Value> {
        match (a, b) {
            (Value::List(x), Value::List(y)) => {
                Some(Value::List([x.clone(), y.clone()].concat()))
            }
            _ => None,
        }
    }
}

impl ComposableType for ListOf {
    fn element_type(&self) -> &dyn NixType {
        &*self.elem_type
    }

    fn with_element(&self, elem: Box<dyn NixType>) -> Box<dyn NixType> {
        Box::new(ListOf::new(elem))
    }
}

/// Attribute set type with element type checking
#[derive(Debug)]
pub struct AttrsOf {
    elem_type: Box<dyn NixType>,
    lazy: bool,
}

impl AttrsOf {
    /// Create a new attrsOf type
    pub fn new(elem_type: Box<dyn NixType>) -> Self {
        Self {
            elem_type,
            lazy: false,
        }
    }

    /// Create a lazy attrsOf type
    pub fn lazy(elem_type: Box<dyn NixType>) -> Self {
        Self {
            elem_type,
            lazy: true,
        }
    }
}

impl Clone for AttrsOf {
    fn clone(&self) -> Self {
        Self {
            elem_type: self.elem_type.clone_box(),
            lazy: self.lazy,
        }
    }
}

impl NixType for AttrsOf {
    fn name(&self) -> &str {
        if self.lazy {
            "lazyAttrsOf"
        } else {
            "attrsOf"
        }
    }

    fn description(&self) -> String {
        let prefix = if self.lazy {
            "lazy attribute set"
        } else {
            "attribute set"
        };
        format!("{} of {}", prefix, self.elem_type.description())
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Attrs(_) => Ok(()), // Shallow check only
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: format!("{:?}", value),
                value: Some(value.clone()),
            }),
        }
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        let defs = self.filter_by_priority(defs);

        if defs.is_empty() {
            return Ok(MergeResult::from_default(Value::Attrs(IndexMap::new())));
        }

        // Collect all attribute definitions by name
        let mut by_name: IndexMap<String, Vec<Definition>> = IndexMap::new();

        for def in defs {
            match &def.value {
                Value::Attrs(attrs) => {
                    for (name, value) in attrs {
                        by_name.entry(name.clone()).or_default().push(Definition {
                            file: def.file.clone(),
                            span: def.span.clone(),
                            value: value.clone(),
                            priority: def.priority,
                        });
                    }
                }
                _ => {
                    return Err(TypeError::Mismatch {
                        expected: self.description(),
                        found: format!("{:?}", def.value),
                        value: Some(def.value.clone()),
                    })
                }
            }
        }

        // Merge each attribute
        let mut result = IndexMap::new();

        for (name, attr_defs) in by_name {
            let attr_loc = loc.child(&name);
            let merged = self.elem_type.merge(&attr_loc, attr_defs)?;

            // For non-lazy attrsOf, skip attributes with empty values from mkIf false
            if !self.lazy {
                if let Some(empty) = self.elem_type.empty_value() {
                    if merged.value == empty && merged.used_default {
                        continue;
                    }
                }
            }

            result.insert(name, merged.value);
        }

        Ok(MergeResult::new(Value::Attrs(result)))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::Attrs(IndexMap::new()))
    }

    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>> {
        let mut map = HashMap::new();
        map.insert("elemType".to_string(), self.elem_type.clone_box());
        map
    }

    fn can_unify(&self, a: &Value, b: &Value) -> bool {
        matches!((a, b), (Value::Attrs(_), Value::Attrs(_)))
    }

    fn unify(&self, a: &Value, b: &Value) -> Option<Value> {
        match (a, b) {
            (Value::Attrs(x), Value::Attrs(y)) => {
                let mut result = x.clone();
                for (k, v) in y {
                    if let Some(existing) = result.get(k) {
                        // Recursively unify
                        if let Some(unified) = self.elem_type.unify(existing, v) {
                            result.insert(k.clone(), unified);
                        } else {
                            return None;
                        }
                    } else {
                        result.insert(k.clone(), v.clone());
                    }
                }
                Some(Value::Attrs(result))
            }
            _ => None,
        }
    }
}

impl ComposableType for AttrsOf {
    fn element_type(&self) -> &dyn NixType {
        &*self.elem_type
    }

    fn with_element(&self, elem: Box<dyn NixType>) -> Box<dyn NixType> {
        Box::new(AttrsOf {
            elem_type: elem,
            lazy: self.lazy,
        })
    }
}

/// Nullable type wrapper
#[derive(Debug)]
pub struct NullOr {
    inner: Box<dyn NixType>,
}

impl NullOr {
    /// Create a new nullOr type
    pub fn new(inner: Box<dyn NixType>) -> Self {
        Self { inner }
    }
}

impl Clone for NullOr {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone_box(),
        }
    }
}

impl NixType for NullOr {
    fn name(&self) -> &str {
        "nullOr"
    }

    fn description(&self) -> String {
        format!("null or {}", self.inner.description())
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Null => Ok(()),
            _ => self.inner.check(value),
        }
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        let defs = self.filter_by_priority(defs);

        if defs.is_empty() {
            return Ok(MergeResult::from_default(Value::Null));
        }

        // Filter out nulls
        let non_null: Vec<_> = defs
            .into_iter()
            .filter(|d| !matches!(d.value, Value::Null))
            .collect();

        if non_null.is_empty() {
            return Ok(MergeResult::new(Value::Null));
        }

        // Merge non-null values with inner type
        self.inner.merge(loc, non_null)
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::Null)
    }

    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>> {
        let mut map = HashMap::new();
        map.insert("inner".to_string(), self.inner.clone_box());
        map
    }
}

/// Enum type with fixed set of values
#[derive(Debug, Clone)]
pub struct Enum {
    values: Vec<String>,
}

impl Enum {
    /// Create a new enum type
    pub fn new(values: Vec<String>) -> Self {
        Self { values }
    }
}

impl NixType for Enum {
    fn name(&self) -> &str {
        "enum"
    }

    fn description(&self) -> String {
        let quoted: Vec<_> = self.values.iter().map(|v| format!("\"{}\"", v)).collect();
        format!("one of {}", quoted.join(", "))
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::String(s) => {
                if self.values.contains(s) {
                    Ok(())
                } else {
                    Err(TypeError::EnumMismatch {
                        expected: self.values.clone(),
                        found: s.clone(),
                    })
                }
            }
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: format!("{:?}", value),
                value: Some(value.clone()),
            }),
        }
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        let defs = self.filter_by_priority(defs);

        if defs.is_empty() {
            return Err(TypeError::NoDefinition {
                path: loc.clone(),
            });
        }

        // All values must be equal
        let first = &defs[0].value;
        for def in &defs[1..] {
            if def.value != *first {
                return Err(TypeError::ConflictingDefinitions {
                    path: loc.clone(),
                    values: defs.iter().map(|d| d.value.clone()).collect(),
                });
            }
        }

        // Validate the value
        self.check(first)?;

        Ok(MergeResult::new(first.clone()))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }
}

/// Union of multiple types
#[derive(Debug)]
pub struct OneOf {
    types: Vec<Box<dyn NixType>>,
}

impl OneOf {
    /// Create a new oneOf type
    pub fn new(types: Vec<Box<dyn NixType>>) -> Self {
        Self { types }
    }
}

impl Clone for OneOf {
    fn clone(&self) -> Self {
        Self {
            types: self.types.iter().map(|t| t.clone_box()).collect(),
        }
    }
}

impl NixType for OneOf {
    fn name(&self) -> &str {
        "oneOf"
    }

    fn description(&self) -> String {
        let descs: Vec<_> = self.types.iter().map(|t| t.description()).collect();
        descs.join(" or ")
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        for ty in &self.types {
            if ty.check(value).is_ok() {
                return Ok(());
            }
        }

        Err(TypeError::Mismatch {
            expected: self.description(),
            found: format!("{:?}", value),
            value: Some(value.clone()),
        })
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        if defs.is_empty() {
            return Err(TypeError::NoDefinition {
                path: loc.clone(),
            });
        }

        // Find which type matches all definitions
        for ty in &self.types {
            if defs.iter().all(|d| ty.check(&d.value).is_ok()) {
                return ty.merge(loc, defs);
            }
        }

        Err(TypeError::Mismatch {
            expected: self.description(),
            found: "mixed types".to_string(),
            value: None,
        })
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>> {
        let mut map = HashMap::new();
        for (i, ty) in self.types.iter().enumerate() {
            map.insert(format!("type{}", i), ty.clone_box());
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Int, Str};

    #[test]
    fn test_list_of_str() {
        let ty = ListOf::new(Box::new(Str));

        // Check valid list
        let value = Value::List(vec![
            Value::String("a".into()),
            Value::String("b".into()),
        ]);
        assert!(ty.check(&value).is_ok());

        // Check non-list
        assert!(ty.check(&Value::String("not a list".into())).is_err());
    }

    #[test]
    fn test_list_merge_concat() {
        let ty = ListOf::new(Box::new(Str));
        let defs = vec![
            Definition::new(Value::List(vec![Value::String("a".into())])),
            Definition::new(Value::List(vec![Value::String("b".into())])),
        ];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();
        assert_eq!(
            result.value,
            Value::List(vec![
                Value::String("a".into()),
                Value::String("b".into()),
            ])
        );
    }

    #[test]
    fn test_attrs_of_merge() {
        let ty = AttrsOf::new(Box::new(Str));

        let mut attrs1 = IndexMap::new();
        attrs1.insert("a".to_string(), Value::String("1".into()));

        let mut attrs2 = IndexMap::new();
        attrs2.insert("b".to_string(), Value::String("2".into()));

        let defs = vec![
            Definition::new(Value::Attrs(attrs1)),
            Definition::new(Value::Attrs(attrs2)),
        ];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.len(), 2);
            assert_eq!(attrs.get("a"), Some(&Value::String("1".into())));
            assert_eq!(attrs.get("b"), Some(&Value::String("2".into())));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_null_or() {
        let ty = NullOr::new(Box::new(Str));

        assert!(ty.check(&Value::Null).is_ok());
        assert!(ty.check(&Value::String("hello".into())).is_ok());
        assert!(ty.check(&Value::Int(42)).is_err());
    }

    #[test]
    fn test_enum() {
        let ty = Enum::new(vec!["foo".into(), "bar".into(), "baz".into()]);

        assert!(ty.check(&Value::String("foo".into())).is_ok());
        assert!(ty.check(&Value::String("invalid".into())).is_err());
    }

    #[test]
    fn test_one_of() {
        let ty = OneOf::new(vec![Box::new(Str), Box::new(Int)]);

        assert!(ty.check(&Value::String("hello".into())).is_ok());
        assert!(ty.check(&Value::Int(42)).is_ok());
        assert!(ty.check(&Value::Bool(true)).is_err());
    }
}
