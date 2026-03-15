//! Topological sorting for module dependencies.

use std::collections::{HashMap, HashSet, VecDeque};

/// Topologically sort nodes by their dependencies
///
/// The `dependencies` map specifies which nodes each node depends on.
/// For example, if `dependencies["b"] = vec!["a"]`, then "b" depends on "a",
/// meaning "a" must come before "b" in the sorted output.
pub fn topological_sort<T: Clone + Eq + std::hash::Hash>(
    nodes: &[T],
    dependencies: &HashMap<T, Vec<T>>,
) -> Result<Vec<T>, TopologicalError<T>> {
    let mut in_degree: HashMap<&T, usize> = HashMap::new();
    // Maps each node to the nodes that depend ON it (reverse dependencies)
    let mut dependents: HashMap<&T, Vec<&T>> = HashMap::new();

    // Initialize in-degrees to 0
    for node in nodes {
        in_degree.entry(node).or_insert(0);
    }

    // Calculate in-degrees based on dependencies
    // If B depends on A, then B's in-degree increases, and A's dependents includes B
    for (node, deps) in dependencies {
        if !nodes.contains(node) {
            continue;
        }
        for dep in deps {
            if nodes.contains(dep) {
                // node depends on dep, so increment node's in-degree
                *in_degree.entry(node).or_insert(0) += 1;
                // dep is a dependency of node, so when dep is processed, node can proceed
                dependents.entry(dep).or_default().push(node);
            }
        }
    }

    // Start with nodes that have no dependencies (in-degree 0)
    let mut queue: VecDeque<&T> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(node, _)| *node)
        .collect();

    let mut result = Vec::new();
    let mut visited = HashSet::new();

    while let Some(node) = queue.pop_front() {
        if visited.contains(node) {
            continue;
        }
        visited.insert(node);
        result.push(node.clone());

        // Process nodes that depend on this one
        if let Some(deps) = dependents.get(node) {
            for dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    // Check for cycles
    if result.len() != nodes.len() {
        let unvisited: Vec<T> = nodes
            .iter()
            .filter(|n| !visited.contains(n))
            .cloned()
            .collect();
        return Err(TopologicalError::Cycle(unvisited));
    }

    Ok(result)
}

/// Errors from topological sorting
#[derive(Debug)]
pub enum TopologicalError<T> {
    /// A cycle was detected involving these nodes
    Cycle(Vec<T>),
}

/// Group nodes by their dependency level (for parallel evaluation)
pub fn group_by_level<T: Clone + Eq + std::hash::Hash>(
    nodes: &[T],
    dependencies: &HashMap<T, Vec<T>>,
) -> Result<Vec<Vec<T>>, TopologicalError<T>> {
    let mut levels: Vec<Vec<T>> = Vec::new();
    let mut placed: HashSet<T> = HashSet::new();
    let mut remaining: Vec<T> = nodes.to_vec();

    while !remaining.is_empty() {
        let mut current_level = Vec::new();

        for node in &remaining {
            let deps = dependencies.get(node).cloned().unwrap_or_default();
            let all_deps_placed = deps.iter().all(|d| placed.contains(d) || !nodes.contains(d));

            if all_deps_placed {
                current_level.push(node.clone());
            }
        }

        if current_level.is_empty() {
            // Cycle detected
            return Err(TopologicalError::Cycle(remaining));
        }

        for node in &current_level {
            placed.insert(node.clone());
        }

        remaining.retain(|n| !current_level.contains(n));
        levels.push(current_level);
    }

    Ok(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_sort() {
        let nodes = vec!["a", "b", "c"];
        let mut deps = HashMap::new();
        deps.insert("b", vec!["a"]);
        deps.insert("c", vec!["b"]);

        let sorted = topological_sort(&nodes, &deps).unwrap();

        let pos_a = sorted.iter().position(|x| *x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| *x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| *x == "c").unwrap();

        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_cycle_detection() {
        let nodes = vec!["a", "b", "c"];
        let mut deps = HashMap::new();
        deps.insert("a", vec!["c"]);
        deps.insert("b", vec!["a"]);
        deps.insert("c", vec!["b"]);

        let result = topological_sort(&nodes, &deps);
        assert!(matches!(result, Err(TopologicalError::Cycle(_))));
    }

    #[test]
    fn test_group_by_level() {
        let nodes = vec!["a", "b", "c", "d"];
        let mut deps = HashMap::new();
        deps.insert("b", vec!["a"]);
        deps.insert("c", vec!["a"]);
        deps.insert("d", vec!["b", "c"]);

        let levels = group_by_level(&nodes, &deps).unwrap();

        assert_eq!(levels.len(), 3);
        assert!(levels[0].contains(&"a"));
        assert!(levels[1].contains(&"b") || levels[1].contains(&"c"));
        assert!(levels[2].contains(&"d"));
    }
}
