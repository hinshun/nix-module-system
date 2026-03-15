//! Merge strategies for different scenarios.

use crate::types::Value;

/// Different strategies for merging values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeStrategy {
    /// All values must be equal (default for primitives)
    Equal,
    /// Use the last value
    Replace,
    /// Concatenate lists
    Concat,
    /// Recursively merge attribute sets
    Recursive,
    /// OR boolean values together
    BoolOr,
    /// AND boolean values together
    BoolAnd,
    /// Take the first non-null value
    FirstNonNull,
}

impl MergeStrategy {
    /// Apply this strategy to merge two values
    pub fn apply(&self, a: &Value, b: &Value) -> Option<Value> {
        match self {
            MergeStrategy::Equal => {
                if a == b {
                    Some(a.clone())
                } else {
                    None
                }
            }

            MergeStrategy::Replace => Some(b.clone()),

            MergeStrategy::Concat => match (a, b) {
                (Value::List(la), Value::List(lb)) => {
                    Some(Value::List([la.clone(), lb.clone()].concat()))
                }
                (Value::String(sa), Value::String(sb)) => {
                    Some(Value::String(format!("{}{}", sa, sb)))
                }
                _ => None,
            },

            MergeStrategy::Recursive => match (a, b) {
                (Value::Attrs(aa), Value::Attrs(ba)) => {
                    let mut result = aa.clone();
                    for (k, v) in ba {
                        if let Some(existing) = result.get(k) {
                            if let Some(merged) = self.apply(existing, v) {
                                result.insert(k.clone(), merged);
                            } else {
                                return None;
                            }
                        } else {
                            result.insert(k.clone(), v.clone());
                        }
                    }
                    Some(Value::Attrs(result))
                }
                _ => MergeStrategy::Equal.apply(a, b),
            },

            MergeStrategy::BoolOr => match (a, b) {
                (Value::Bool(ba), Value::Bool(bb)) => Some(Value::Bool(*ba || *bb)),
                _ => None,
            },

            MergeStrategy::BoolAnd => match (a, b) {
                (Value::Bool(ba), Value::Bool(bb)) => Some(Value::Bool(*ba && *bb)),
                _ => None,
            },

            MergeStrategy::FirstNonNull => {
                if !matches!(a, Value::Null) {
                    Some(a.clone())
                } else {
                    Some(b.clone())
                }
            }
        }
    }

    /// Merge multiple values using this strategy
    pub fn apply_all(&self, values: impl IntoIterator<Item = Value>) -> Option<Value> {
        let mut iter = values.into_iter();
        let first = iter.next()?;

        iter.try_fold(first, |acc, val| self.apply(&acc, &val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_equal_strategy() {
        let strategy = MergeStrategy::Equal;

        assert!(strategy.apply(&Value::Int(1), &Value::Int(1)).is_some());
        assert!(strategy.apply(&Value::Int(1), &Value::Int(2)).is_none());
    }

    #[test]
    fn test_replace_strategy() {
        let strategy = MergeStrategy::Replace;

        let result = strategy.apply(&Value::Int(1), &Value::Int(2));
        assert_eq!(result, Some(Value::Int(2)));
    }

    #[test]
    fn test_concat_strategy() {
        let strategy = MergeStrategy::Concat;

        let a = Value::List(vec![Value::Int(1)]);
        let b = Value::List(vec![Value::Int(2)]);

        let result = strategy.apply(&a, &b).unwrap();
        assert_eq!(result, Value::List(vec![Value::Int(1), Value::Int(2)]));
    }

    #[test]
    fn test_recursive_strategy() {
        let strategy = MergeStrategy::Recursive;

        let mut a = IndexMap::new();
        a.insert("x".to_string(), Value::Int(1));

        let mut b = IndexMap::new();
        b.insert("y".to_string(), Value::Int(2));

        let result = strategy
            .apply(&Value::Attrs(a), &Value::Attrs(b))
            .unwrap();

        if let Value::Attrs(attrs) = result {
            assert_eq!(attrs.len(), 2);
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_bool_or_strategy() {
        let strategy = MergeStrategy::BoolOr;

        let result = strategy.apply(&Value::Bool(false), &Value::Bool(true));
        assert_eq!(result, Some(Value::Bool(true)));

        let result = strategy.apply(&Value::Bool(false), &Value::Bool(false));
        assert_eq!(result, Some(Value::Bool(false)));
    }

    #[test]
    fn test_apply_all() {
        let strategy = MergeStrategy::Concat;

        let values = vec![
            Value::List(vec![Value::Int(1)]),
            Value::List(vec![Value::Int(2)]),
            Value::List(vec![Value::Int(3)]),
        ];

        let result = strategy.apply_all(values).unwrap();
        assert_eq!(
            result,
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }
}
