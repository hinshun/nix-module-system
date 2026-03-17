//! Demonstration of working functionality in the Nix module system.
//!
//! This example shows the real, working components:
//! - Type checking (Bool, Int, Str types)
//! - Merge engine with priorities
//! - mkIf / mkMerge conditional handling
//!
//! Run with: cargo run --example real_eval_demo

use nix_module_system::merge::{mk_if, mk_merge, MergeEngine};
use nix_module_system::types::{Bool, Definition, Int, NixType, OptionPath, Str, Value};

fn main() {
    println!("Nix Module System - Working Components Demo\n");

    demo_type_checking();
    println!();
    demo_merge_engine();
    println!();
    demo_conditionals();
    println!();
    show_architecture();
}

fn demo_type_checking() {
    println!("=== Type Checking ===\n");

    let bool_type = Bool;
    let int_type = Int;
    let str_type = Str;

    let bool_val = Value::Bool(true);
    println!(
        "  Bool.check(true)  = {:?}",
        bool_type.check(&bool_val).is_ok()
    );

    let int_val = Value::Int(443);
    println!(
        "  Int.check(443)    = {:?}",
        int_type.check(&int_val).is_ok()
    );

    let str_val = Value::String("hello".into());
    println!(
        "  Str.check(\"hello\") = {:?}",
        str_type.check(&str_val).is_ok()
    );

    println!("\nType mismatches:");
    let wrong = Value::String("443".into());
    match int_type.check(&wrong) {
        Ok(_) => println!("  Int.check(\"443\") = ok (unexpected)"),
        Err(e) => println!("  Int.check(\"443\") = Err: {}", e),
    }
}

fn demo_merge_engine() {
    println!("=== Merge Engine with Priorities ===\n");

    let mut engine = MergeEngine::new();
    let path = OptionPath::from_dotted("services.nginx.port");

    let defs = vec![
        Definition::with_priority(Value::Int(80), 1000),  // mkDefault
        Definition::with_priority(Value::Int(443), 100),   // normal
        Definition::with_priority(Value::Int(8080), 50),   // mkForce
    ];

    println!("  mkDefault: 80   (priority 1000)");
    println!("  normal:    443  (priority 100)");
    println!("  mkForce:   8080 (priority 50)");

    match engine.merge_option(&Int, &path, defs) {
        Ok(merged) => println!("\n  Result: {} (mkForce wins)", merged.value),
        Err(e) => println!("\n  Merge error: {}", e),
    }

    println!("\nConflicting string values:");
    let str_defs = vec![
        Definition::new(Value::String("foo".into())),
        Definition::new(Value::String("bar".into())),
    ];
    let path2 = OptionPath::from_dotted("test.option");
    match engine.merge_option(&Str, &path2, str_defs) {
        Ok(merged) => println!("  Merged: {}", merged.value),
        Err(e) => println!("  Conflict error: {}", e),
    }
}

fn demo_conditionals() {
    println!("=== mkIf / mkMerge Conditionals ===\n");

    let mut engine = MergeEngine::new();

    let optimisation = true;
    let gzip = true;
    let tls = true;

    println!("  optimisation={}, gzip={}, tls={}\n", optimisation, gzip, tls);

    let merged_val = mk_merge(vec![
        mk_if(optimisation, Value::String("sendfile on;".into())),
        mk_if(gzip, Value::String("gzip on;".into())),
        mk_if(!tls, Value::String("# no tls".into())),
    ]);

    let defs = vec![Definition::new(merged_val)];
    let filtered = engine.filter_conditional(defs);

    println!("  After filtering:");
    for (i, def) in filtered.iter().enumerate() {
        println!("    [{}] {}", i, def.value);
    }
    println!("\n  mkIf false (# no tls) was correctly filtered out!");
}

fn show_architecture() {
    println!("=== Architecture ===\n");
    println!("  Nix evaluator (in charge)");
    println!("    -> nix/lib.nix: evalModules, mkOption, types.*");
    println!("    -> Rust primops (via nix-module-plugin):");
    println!("       - __nms_mergeDefinitions: merge engine");
    println!("       - __nms_checkType: type checking");
    println!("       - __nms_processConditionals: mkIf/mkMerge");
    println!("       - __nms_version: plugin version");
}
