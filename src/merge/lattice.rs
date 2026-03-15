//! Lattice-based unification for order-independent merging.
//!
//! This module implements the core insight from CUE: values form a lattice
//! where unification is the meet operation. This enables:
//! - Order-independent evaluation
//! - Parallel processing of modules
//! - Early conflict detection

#![allow(missing_docs)]

use crate::types::Value;
use indexmap::IndexMap;

/// Trait for values that can be unified in a lattice
pub trait Unify: Sized {
    /// Unify two values, returning None if they are incompatible
    fn unify(&self, other: &Self) -> Option<Self>;

    /// Check if unification would succeed without computing the result
    fn can_unify(&self, other: &Self) -> bool {
        self.unify(other).is_some()
    }
}

impl Unify for Value {
    fn unify(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            // Nulls unify
            (Value::Null, Value::Null) => Some(Value::Null),

            // Equal primitives unify
            (Value::Bool(a), Value::Bool(b)) if a == b => Some(self.clone()),
            (Value::Int(a), Value::Int(b)) if a == b => Some(self.clone()),
            (Value::Float(a), Value::Float(b)) if a == b => Some(self.clone()),
            (Value::String(a), Value::String(b)) if a == b => Some(self.clone()),
            (Value::Path(a), Value::Path(b)) if a == b => Some(self.clone()),

            // Lists concatenate
            (Value::List(a), Value::List(b)) => {
                Some(Value::List([a.clone(), b.clone()].concat()))
            }

            // Attrs merge recursively
            (Value::Attrs(a), Value::Attrs(b)) => {
                unify_attrs(a, b).map(Value::Attrs)
            }

            // Derivations are compared by their attrs
            (Value::Derivation(a), Value::Derivation(b)) => {
                match (a.as_ref(), b.as_ref()) {
                    (Value::Attrs(aa), Value::Attrs(ba)) => {
                        unify_attrs(aa, ba).map(|attrs| Value::Derivation(Box::new(Value::Attrs(attrs))))
                    }
                    _ => None,
                }
            }

            // Incompatible types or values
            _ => None,
        }
    }
}

/// Unify two attribute sets
fn unify_attrs(
    a: &IndexMap<String, Value>,
    b: &IndexMap<String, Value>,
) -> Option<IndexMap<String, Value>> {
    let mut result = a.clone();

    for (key, b_val) in b {
        if let Some(a_val) = result.get(key) {
            // Key exists in both - must unify
            let unified = a_val.unify(b_val)?;
            result.insert(key.clone(), unified);
        } else {
            // Key only in b - add it
            result.insert(key.clone(), b_val.clone());
        }
    }

    Some(result)
}

/// Unify multiple values in order
pub fn unify_all<T: Unify + Clone>(values: impl IntoIterator<Item = T>) -> Option<T> {
    let mut iter = values.into_iter();
    let first = iter.next()?;

    iter.try_fold(first, |acc, val| acc.unify(&val))
}

/// A lattice element that tracks conflicts
#[derive(Debug, Clone)]
pub enum LatticeValue<T> {
    /// A concrete value
    Value(T),
    /// Top element - represents "any value"
    Top,
    /// Bottom element - represents a conflict
    Bottom { reason: String },
}

impl<T: Unify + Clone> Unify for LatticeValue<T> {
    fn unify(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            // Bottom propagates
            (LatticeValue::Bottom { .. }, _) => Some(self.clone()),
            (_, LatticeValue::Bottom { .. }) => Some(other.clone()),

            // Top is identity
            (LatticeValue::Top, x) | (x, LatticeValue::Top) => Some(x.clone()),

            // Values unify normally
            (LatticeValue::Value(a), LatticeValue::Value(b)) => {
                a.unify(b).map(LatticeValue::Value)
            }
        }
    }
}

impl<T> Default for LatticeValue<T> {
    fn default() -> Self {
        LatticeValue::Top
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unify_primitives() {
        // Equal values unify
        let a = Value::Int(42);
        let b = Value::Int(42);
        assert_eq!(a.unify(&b), Some(Value::Int(42)));

        // Unequal values don't unify
        let c = Value::Int(43);
        assert_eq!(a.unify(&c), None);
    }

    #[test]
    fn test_unify_lists() {
        let a = Value::List(vec![Value::Int(1)]);
        let b = Value::List(vec![Value::Int(2)]);

        let result = a.unify(&b).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn test_unify_attrs() {
        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));

        let mut b = IndexMap::new();
        b.insert("y".to_string(), Value::Int(2));

        let a_val = Value::Attrs(a);
        let b_val = Value::Attrs(b);

        let result = a_val.unify(&b_val).unwrap();

        if let Value::Attrs(attrs) = result {
            assert_eq!(attrs.len(), 2);
            assert_eq!(attrs.get("x"), Some(&Value::Int(1)));
            assert_eq!(attrs.get("y"), Some(&Value::Int(2)));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_unify_attrs_conflict() {
        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));

        let mut b = IndexMap::new();
        b.insert("x".to_string(), Value::Int(2)); // Conflict!

        let a_val = Value::Attrs(a);
        let b_val = Value::Attrs(b);

        assert!(a_val.unify(&b_val).is_none());
    }

    #[test]
    fn test_unify_all() {
        let values = vec![
            Value::Int(42),
            Value::Int(42),
            Value::Int(42),
        ];

        assert_eq!(unify_all(values), Some(Value::Int(42)));
    }

    #[test]
    fn test_lattice_value() {
        let a: LatticeValue<Value> = LatticeValue::Value(Value::Int(42));
        let b: LatticeValue<Value> = LatticeValue::Top;

        let result = a.unify(&b).unwrap();
        assert!(matches!(result, LatticeValue::Value(Value::Int(42))));
    }
}
