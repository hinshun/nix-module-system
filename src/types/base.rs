//! Primitive types: str, bool, int, float, path

use super::{Definition, MergeResult, NixType, OptionPath, PriorityMerge, Value};
use crate::errors::TypeError;

/// String type
#[derive(Debug, Clone)]
pub struct Str;

impl NixType for Str {
    fn name(&self) -> &str {
        "str"
    }

    fn description(&self) -> String {
        "string".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::String(_) => Ok(()),
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: value_type_name(value),
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

        // All strings must be equal
        let first = &defs[0].value;
        for def in &defs[1..] {
            if def.value != *first {
                return Err(TypeError::ConflictingDefinitions {
                    path: loc.clone(),
                    values: defs.iter().map(|d| d.value.clone()).collect(),
                });
            }
        }

        Ok(MergeResult::new(first.clone()))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::String(String::new()))
    }
}

/// Boolean type
#[derive(Debug, Clone)]
pub struct Bool;

impl NixType for Bool {
    fn name(&self) -> &str {
        "bool"
    }

    fn description(&self) -> String {
        "boolean".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Bool(_) => Ok(()),
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: value_type_name(value),
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

        // All bools must be equal
        let first = &defs[0].value;
        for def in &defs[1..] {
            if def.value != *first {
                return Err(TypeError::ConflictingDefinitions {
                    path: loc.clone(),
                    values: defs.iter().map(|d| d.value.clone()).collect(),
                });
            }
        }

        Ok(MergeResult::new(first.clone()))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::Bool(false))
    }
}

/// Boolean type that merges with OR
#[derive(Debug, Clone)]
pub struct BoolByOr;

impl NixType for BoolByOr {
    fn name(&self) -> &str {
        "boolByOr"
    }

    fn description(&self) -> String {
        "boolean (merged using or)".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        Bool.check(value)
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        let defs = self.filter_by_priority(defs);

        if defs.is_empty() {
            return Err(TypeError::NoDefinition {
                path: loc.clone(),
            });
        }

        // OR all values together
        let mut result = false;
        for def in &defs {
            match &def.value {
                Value::Bool(b) => result = result || *b,
                _ => {
                    return Err(TypeError::Mismatch {
                        expected: self.description(),
                        found: value_type_name(&def.value),
                        value: Some(def.value.clone()),
                    })
                }
            }
        }

        Ok(MergeResult::new(Value::Bool(result)))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::Bool(false))
    }

    fn can_unify(&self, a: &Value, b: &Value) -> bool {
        // BoolByOr can always unify bools
        matches!((a, b), (Value::Bool(_), Value::Bool(_)))
    }

    fn unify(&self, a: &Value, b: &Value) -> Option<Value> {
        match (a, b) {
            (Value::Bool(x), Value::Bool(y)) => Some(Value::Bool(*x || *y)),
            _ => None,
        }
    }
}

/// Integer type
#[derive(Debug, Clone)]
pub struct Int;

impl NixType for Int {
    fn name(&self) -> &str {
        "int"
    }

    fn description(&self) -> String {
        "signed integer".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Int(_) => Ok(()),
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: value_type_name(value),
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

        // All ints must be equal
        let first = &defs[0].value;
        for def in &defs[1..] {
            if def.value != *first {
                return Err(TypeError::ConflictingDefinitions {
                    path: loc.clone(),
                    values: defs.iter().map(|d| d.value.clone()).collect(),
                });
            }
        }

        Ok(MergeResult::new(first.clone()))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::Int(0))
    }
}

/// Float type
#[derive(Debug, Clone)]
pub struct Float;

impl NixType for Float {
    fn name(&self) -> &str {
        "float"
    }

    fn description(&self) -> String {
        "floating point number".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Float(_) => Ok(()),
            Value::Int(_) => Ok(()), // Ints are coerced to floats
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: value_type_name(value),
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

        // All floats must be equal
        let first = &defs[0].value;
        for def in &defs[1..] {
            if def.value != *first {
                return Err(TypeError::ConflictingDefinitions {
                    path: loc.clone(),
                    values: defs.iter().map(|d| d.value.clone()).collect(),
                });
            }
        }

        Ok(MergeResult::new(first.clone()))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }
}

/// Path type
#[derive(Debug, Clone)]
pub struct Path;

impl NixType for Path {
    fn name(&self) -> &str {
        "path"
    }

    fn description(&self) -> String {
        "path".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Path(_) => Ok(()),
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: value_type_name(value),
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

        // All paths must be equal
        let first = &defs[0].value;
        for def in &defs[1..] {
            if def.value != *first {
                return Err(TypeError::ConflictingDefinitions {
                    path: loc.clone(),
                    values: defs.iter().map(|d| d.value.clone()).collect(),
                });
            }
        }

        Ok(MergeResult::new(first.clone()))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }
}

/// Helper to get type name for a value
fn value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Int(_) => "int".to_string(),
        Value::Float(_) => "float".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Path(_) => "path".to_string(),
        Value::List(_) => "list".to_string(),
        Value::Attrs(_) => "attribute set".to_string(),
        Value::Lambda => "function".to_string(),
        Value::Derivation(_) => "derivation".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_str_check() {
        let ty = Str;
        assert!(ty.check(&Value::String("hello".into())).is_ok());
        assert!(ty.check(&Value::Int(42)).is_err());
    }

    #[test]
    fn test_str_merge_equal() {
        let ty = Str;
        let defs = vec![
            Definition::new(Value::String("hello".into())),
            Definition::new(Value::String("hello".into())),
        ];
        let result = ty.merge(&OptionPath::root(), defs).unwrap();
        assert_eq!(result.value, Value::String("hello".into()));
    }

    #[test]
    fn test_str_merge_conflict() {
        let ty = Str;
        let defs = vec![
            Definition::new(Value::String("hello".into())),
            Definition::new(Value::String("world".into())),
        ];
        assert!(ty.merge(&OptionPath::root(), defs).is_err());
    }

    #[test]
    fn test_bool_by_or() {
        let ty = BoolByOr;
        let defs = vec![
            Definition::new(Value::Bool(false)),
            Definition::new(Value::Bool(true)),
            Definition::new(Value::Bool(false)),
        ];
        let result = ty.merge(&OptionPath::root(), defs).unwrap();
        assert_eq!(result.value, Value::Bool(true));
    }

    #[test]
    fn test_int_check() {
        let ty = Int;
        assert!(ty.check(&Value::Int(42)).is_ok());
        assert!(ty.check(&Value::String("42".into())).is_err());
    }

    #[test]
    fn test_priority_merge() {
        let ty = Str;
        let defs = vec![
            Definition::with_priority(Value::String("default".into()), 1000), // mkDefault
            Definition::with_priority(Value::String("forced".into()), 50),    // mkForce
        ];
        let result = ty.merge(&OptionPath::root(), defs).unwrap();
        assert_eq!(result.value, Value::String("forced".into()));
    }
}
