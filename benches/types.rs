//! Benchmarks for the type system.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use indexmap::IndexMap;
use nix_module_system::types::{
    base::{Bool, Int, Str},
    compound::{AttrsOf, ListOf, NullOr},
    NixType, OptionPath, Value,
};

fn bench_type_checking(c: &mut Criterion) {
    c.bench_function("check_string_type", |b| {
        let str_type = Str;
        let value = Value::String("hello world".to_string());
        b.iter(|| str_type.check(black_box(&value)))
    });

    c.bench_function("check_int_type", |b| {
        let int_type = Int;
        let value = Value::Int(42);
        b.iter(|| int_type.check(black_box(&value)))
    });

    c.bench_function("check_bool_type", |b| {
        let bool_type = Bool;
        let value = Value::Bool(true);
        b.iter(|| bool_type.check(black_box(&value)))
    });

    c.bench_function("check_string_type_failure", |b| {
        let str_type = Str;
        let value = Value::Int(42);
        b.iter(|| str_type.check(black_box(&value)))
    });
}

fn bench_compound_types(c: &mut Criterion) {
    c.bench_function("check_list_of_int_small", |b| {
        let list_type = ListOf::new(Box::new(Int));
        let value = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        b.iter(|| list_type.check(black_box(&value)))
    });

    c.bench_function("check_list_of_int_large", |b| {
        let list_type = ListOf::new(Box::new(Int));
        let value = Value::List((0..100).map(Value::Int).collect());
        b.iter(|| list_type.check(black_box(&value)))
    });

    c.bench_function("check_attrs_of_string_small", |b| {
        let attrs_type = AttrsOf::new(Box::new(Str));
        let mut attrs = IndexMap::new();
        attrs.insert("a".to_string(), Value::String("hello".to_string()));
        attrs.insert("b".to_string(), Value::String("world".to_string()));
        let value = Value::Attrs(attrs);
        b.iter(|| attrs_type.check(black_box(&value)))
    });

    c.bench_function("check_attrs_of_string_large", |b| {
        let attrs_type = AttrsOf::new(Box::new(Str));
        let mut attrs = IndexMap::new();
        for i in 0..100 {
            attrs.insert(format!("key_{}", i), Value::String(format!("value_{}", i)));
        }
        let value = Value::Attrs(attrs);
        b.iter(|| attrs_type.check(black_box(&value)))
    });
}

fn bench_nullable_types(c: &mut Criterion) {
    c.bench_function("check_null_or_string_with_null", |b| {
        let nullable = NullOr::new(Box::new(Str));
        let value = Value::Null;
        b.iter(|| nullable.check(black_box(&value)))
    });

    c.bench_function("check_null_or_string_with_string", |b| {
        let nullable = NullOr::new(Box::new(Str));
        let value = Value::String("hello".to_string());
        b.iter(|| nullable.check(black_box(&value)))
    });
}

fn bench_nested_types(c: &mut Criterion) {
    c.bench_function("check_nested_attrs_of_list_of_int", |b| {
        let nested_type = AttrsOf::new(Box::new(ListOf::new(Box::new(Int))));

        let mut attrs = IndexMap::new();
        for i in 0..10 {
            attrs.insert(
                format!("list_{}", i),
                Value::List((0..10).map(Value::Int).collect()),
            );
        }
        let value = Value::Attrs(attrs);

        b.iter(|| nested_type.check(black_box(&value)))
    });

    c.bench_function("check_deeply_nested_type", |b| {
        // attrs of (nullable (list of (attrs of string)))
        let deep_type = AttrsOf::new(Box::new(NullOr::new(Box::new(ListOf::new(Box::new(
            AttrsOf::new(Box::new(Str)),
        ))))));

        let mut inner_attrs = IndexMap::new();
        inner_attrs.insert("name".to_string(), Value::String("test".to_string()));

        let list = Value::List(vec![Value::Attrs(inner_attrs.clone())]);

        let mut outer_attrs = IndexMap::new();
        outer_attrs.insert("data".to_string(), list);

        let value = Value::Attrs(outer_attrs);

        b.iter(|| deep_type.check(black_box(&value)))
    });
}

fn bench_type_merge(c: &mut Criterion) {
    use nix_module_system::merge::{Definition, Priority};

    fn create_def(value: Value, priority: Priority) -> Definition {
        Definition {
            value,
            priority,
            file: None,
            line: None,
        }
    }

    c.bench_function("merge_list_of_int", |b| {
        let list_type = ListOf::new(Box::new(Int));
        let path = OptionPath::new(vec!["packages".to_string()]);
        let defs = vec![
            create_def(
                Value::List(vec![Value::Int(1), Value::Int(2)]),
                Priority::Normal,
            ),
            create_def(
                Value::List(vec![Value::Int(3), Value::Int(4)]),
                Priority::Normal,
            ),
        ];

        b.iter(|| list_type.merge(black_box(&path), black_box(defs.clone())))
    });

    c.bench_function("merge_attrs_of_string", |b| {
        let attrs_type = AttrsOf::new(Box::new(Str));
        let path = OptionPath::new(vec!["config".to_string()]);

        let mut attrs1 = IndexMap::new();
        attrs1.insert("a".to_string(), Value::String("x".to_string()));

        let mut attrs2 = IndexMap::new();
        attrs2.insert("b".to_string(), Value::String("y".to_string()));

        let defs = vec![
            create_def(Value::Attrs(attrs1), Priority::Normal),
            create_def(Value::Attrs(attrs2), Priority::Normal),
        ];

        b.iter(|| attrs_type.merge(black_box(&path), black_box(defs.clone())))
    });
}

criterion_group!(
    benches,
    bench_type_checking,
    bench_compound_types,
    bench_nullable_types,
    bench_nested_types,
    bench_type_merge
);
criterion_main!(benches);
