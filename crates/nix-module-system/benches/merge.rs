//! Benchmarks for the merge engine.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use indexmap::IndexMap;
use nix_module_system::merge::{Definition, MergeEngine, Priority};
use nix_module_system::types::{OptionPath, Value};

fn create_simple_definition(value: Value, priority: Priority) -> Definition {
    Definition {
        value,
        priority,
        file: None,
        line: None,
    }
}

fn bench_merge_primitives(c: &mut Criterion) {
    let engine = MergeEngine::new();
    let path = OptionPath::new(vec!["test".to_string()]);

    c.bench_function("merge_single_string", |b| {
        let defs = vec![create_simple_definition(
            Value::String("hello".to_string()),
            Priority::Normal,
        )];
        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });

    c.bench_function("merge_two_strings_same_priority", |b| {
        let defs = vec![
            create_simple_definition(Value::String("hello".to_string()), Priority::Normal),
            create_simple_definition(Value::String("hello".to_string()), Priority::Normal),
        ];
        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });

    c.bench_function("merge_with_override", |b| {
        let defs = vec![
            create_simple_definition(Value::String("default".to_string()), Priority::Default),
            create_simple_definition(Value::String("override".to_string()), Priority::Force),
        ];
        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });
}

fn bench_merge_attrs(c: &mut Criterion) {
    let engine = MergeEngine::new();
    let path = OptionPath::new(vec!["config".to_string()]);

    c.bench_function("merge_small_attrs", |b| {
        let mut attrs1 = IndexMap::new();
        attrs1.insert("a".to_string(), Value::Int(1));
        attrs1.insert("b".to_string(), Value::Int(2));

        let mut attrs2 = IndexMap::new();
        attrs2.insert("c".to_string(), Value::Int(3));
        attrs2.insert("d".to_string(), Value::Int(4));

        let defs = vec![
            create_simple_definition(Value::Attrs(attrs1), Priority::Normal),
            create_simple_definition(Value::Attrs(attrs2), Priority::Normal),
        ];

        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });

    c.bench_function("merge_large_attrs", |b| {
        let mut attrs1 = IndexMap::new();
        let mut attrs2 = IndexMap::new();

        for i in 0..100 {
            attrs1.insert(format!("key_{}", i), Value::Int(i));
            attrs2.insert(format!("key_{}", i + 100), Value::Int(i + 100));
        }

        let defs = vec![
            create_simple_definition(Value::Attrs(attrs1), Priority::Normal),
            create_simple_definition(Value::Attrs(attrs2), Priority::Normal),
        ];

        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });
}

fn bench_merge_lists(c: &mut Criterion) {
    let engine = MergeEngine::new();
    let path = OptionPath::new(vec!["packages".to_string()]);

    c.bench_function("merge_small_lists", |b| {
        let list1 = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let list2 = Value::List(vec![Value::Int(4), Value::Int(5), Value::Int(6)]);

        let defs = vec![
            create_simple_definition(list1, Priority::Normal),
            create_simple_definition(list2, Priority::Normal),
        ];

        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });

    c.bench_function("merge_large_lists", |b| {
        let list1 = Value::List((0..100).map(Value::Int).collect());
        let list2 = Value::List((100..200).map(Value::Int).collect());

        let defs = vec![
            create_simple_definition(list1, Priority::Normal),
            create_simple_definition(list2, Priority::Normal),
        ];

        b.iter(|| {
            engine
                .merge_definitions(black_box(&path), black_box(defs.clone()))
                .unwrap()
        })
    });
}

criterion_group!(
    benches,
    bench_merge_primitives,
    bench_merge_attrs,
    bench_merge_lists
);
criterion_main!(benches);
