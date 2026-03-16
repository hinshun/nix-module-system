//! Priority handling for mkOverride, mkDefault, mkForce.

use crate::types::{Definition, Value};

/// Process a value that may have override markers
pub fn process_priority(value: &Value) -> (Value, i32) {
    // In a real implementation, this would inspect the value
    // for _type = "override" markers and extract the priority.
    // For now, return the value as-is with default priority.
    (value.clone(), 100)
}

/// Create an mkDefault wrapper (priority 1000)
pub fn mk_default(value: Value) -> (Value, i32) {
    (value, 1000)
}

/// Create an mkForce wrapper (priority 50)
pub fn mk_force(value: Value) -> (Value, i32) {
    (value, 50)
}

/// Create an mkOverride wrapper with custom priority
pub fn mk_override(priority: i32, value: Value) -> (Value, i32) {
    (value, priority)
}

/// Filter and sort definitions by priority
pub fn order_by_priority(mut defs: Vec<Definition>) -> Vec<Definition> {
    defs.sort_by_key(|d| d.priority);
    defs
}

/// Group definitions by their priority level
pub fn group_by_priority(defs: Vec<Definition>) -> Vec<(i32, Vec<Definition>)> {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<i32, Vec<Definition>> = BTreeMap::new();

    for def in defs {
        groups.entry(def.priority).or_default().push(def);
    }

    groups.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mk_default() {
        let (_, priority) = mk_default(Value::Bool(true));
        assert_eq!(priority, 1000);
    }

    #[test]
    fn test_mk_force() {
        let (_, priority) = mk_force(Value::Bool(true));
        assert_eq!(priority, 50);
    }

    #[test]
    fn test_order_by_priority() {
        let defs = vec![
            Definition::with_priority(Value::Int(1), 1000),
            Definition::with_priority(Value::Int(2), 50),
            Definition::with_priority(Value::Int(3), 100),
        ];

        let ordered = order_by_priority(defs);
        assert_eq!(ordered[0].priority, 50);
        assert_eq!(ordered[1].priority, 100);
        assert_eq!(ordered[2].priority, 1000);
    }

    #[test]
    fn test_group_by_priority() {
        let defs = vec![
            Definition::with_priority(Value::Int(1), 100),
            Definition::with_priority(Value::Int(2), 50),
            Definition::with_priority(Value::Int(3), 100),
        ];

        let groups = group_by_priority(defs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, 50);
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[1].0, 100);
        assert_eq!(groups[1].1.len(), 2);
    }
}
