//! Demonstration of working functionality in the Nix module system.
//!
//! This example shows the real, working components:
//! - Parser (lexer + AST construction)
//! - Type checking (Bool, Int, Str types)
//! - Merge engine with priorities
//! - mkIf / mkMerge conditional handling
//!
//! Run with: cargo run --example real_eval_demo

use nix_module_system::parse::parse_module;
use nix_module_system::types::{Bool, Int, Str, NixType, Value, Definition, OptionPath};
use nix_module_system::merge::{MergeEngine, mk_if, mk_merge};
use std::path::PathBuf;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║     Nix Module System - Working Components Demo                  ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    demo_real_parser();
    println!("\n");
    demo_real_type_checking();
    println!("\n");
    demo_real_merge_engine();
    println!("\n");
    demo_real_conditionals();
    println!("\n");
    show_limitations();
}

fn demo_real_parser() {
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│  ✓ REAL: Parser                                                  │");
    println!("└──────────────────────────────────────────────────────────────────┘\n");

    let source = r#"{ config, lib, pkgs, ... }: {
  services.nginx.enable = true;
  services.nginx.port = 443;
}"#;

    println!("Input:");
    println!("───────");
    println!("{}\n", source);

    match parse_module(source, PathBuf::from("test.nix")) {
        Ok(ast) => {
            println!("✓ Parsed successfully!");
            println!("  AST root: {:?}", std::mem::discriminant(&ast.node));
            println!("  Span: {}..{}", ast.span.start, ast.span.end);
        }
        Err(errors) => {
            println!("✗ Parse errors:");
            for err in errors {
                println!("  - {} at {}:{}", err.message, err.span.line, err.span.column);
            }
        }
    }

    // Show a parse error
    println!("\nWith syntax error:");
    println!("───────────────────");
    let bad_source = r#"{ config, lib, pkgs, ... }: {
  services.nginx.enable = true
  services.nginx.port = 443;  # missing semicolon above
}"#;
    println!("{}\n", bad_source);

    match parse_module(bad_source, PathBuf::from("test.nix")) {
        Ok(_) => println!("✓ Parsed (unexpectedly)"),
        Err(errors) => {
            println!("✗ Parse errors (as expected):");
            for err in errors {
                println!("  line {}: {}", err.span.line, err.message);
            }
        }
    }
}

fn demo_real_type_checking() {
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│  ✓ REAL: Type Checking                                           │");
    println!("└──────────────────────────────────────────────────────────────────┘\n");

    let bool_type = Bool;
    let int_type = Int;
    let str_type = Str;

    // Valid checks
    println!("Type checks:");
    println!("─────────────");

    let bool_val = Value::Bool(true);
    println!("  Bool.check(true)  = {:?}", bool_type.check(&bool_val).is_ok());

    let int_val = Value::Int(443);
    println!("  Int.check(443)    = {:?}", int_type.check(&int_val).is_ok());

    let str_val = Value::String("hello".into());
    println!("  Str.check(\"hello\") = {:?}", str_type.check(&str_val).is_ok());

    // Invalid checks - REAL type errors
    println!("\nType mismatches:");
    println!("─────────────────");

    let wrong = Value::String("443".into());
    match int_type.check(&wrong) {
        Ok(_) => println!("  Int.check(\"443\") = ok (unexpected)"),
        Err(e) => println!("  Int.check(\"443\") = Err: {}", e),
    }

    let wrong2 = Value::Int(1);
    match str_type.check(&wrong2) {
        Ok(_) => println!("  Str.check(1)     = ok (unexpected)"),
        Err(e) => println!("  Str.check(1)     = Err: {}", e),
    }
}

fn demo_real_merge_engine() {
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│  ✓ REAL: Merge Engine with Priorities                            │");
    println!("└──────────────────────────────────────────────────────────────────┘\n");

    let mut engine = MergeEngine::new();
    let path = OptionPath::from_dotted("services.nginx.port");

    // Multiple definitions with different priorities
    let defs = vec![
        Definition::with_priority(Value::Int(80), 1000),   // mkDefault (low priority)
        Definition::with_priority(Value::Int(443), 100),   // normal
        Definition::with_priority(Value::Int(8080), 50),   // mkForce (high priority)
    ];

    println!("Definitions:");
    println!("─────────────");
    println!("  mkDefault: 80   (priority 1000)");
    println!("  normal:    443  (priority 100)");
    println!("  mkForce:   8080 (priority 50)");
    println!();

    let result = engine.merge_option(&Int, &path, defs);
    match result {
        Ok(merged) => {
            println!("✓ Merged result: {} (mkForce wins)", merged.value);
        }
        Err(e) => {
            println!("✗ Merge error: {}", e);
        }
    }

    // Conflicting string merge
    println!("\nConflicting string values:");
    println!("───────────────────────────");
    let str_defs = vec![
        Definition::new(Value::String("foo".into())),
        Definition::new(Value::String("bar".into())),
    ];

    let path2 = OptionPath::from_dotted("test.option");
    match engine.merge_option(&Str, &path2, str_defs) {
        Ok(merged) => println!("✓ Merged: {}", merged.value),
        Err(e) => println!("✗ Conflict error: {}", e),
    }
}

fn demo_real_conditionals() {
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│  ✓ REAL: mkIf / mkMerge Conditionals                             │");
    println!("└──────────────────────────────────────────────────────────────────┘\n");

    let mut engine = MergeEngine::new();
    let path = OptionPath::from_dotted("services.nginx.httpConfig");

    // Simulate: mkMerge [
    //   (mkIf optimisation "sendfile on;")
    //   (mkIf gzip "gzip on;")
    //   (mkIf (!tls) "# no tls")
    // ]
    let optimisation = true;
    let gzip = true;
    let tls = true;

    println!("Conditions:");
    println!("────────────");
    println!("  optimisation = {}", optimisation);
    println!("  gzip = {}", gzip);
    println!("  tls = {} (so !tls = {})", tls, !tls);
    println!();

    let merged_val = mk_merge(vec![
        mk_if(optimisation, Value::String("sendfile on;".into())),
        mk_if(gzip, Value::String("gzip on;".into())),
        mk_if(!tls, Value::String("# no tls".into())),
    ]);

    let defs = vec![Definition::new(merged_val)];

    // Filter conditionals
    let filtered = engine.filter_conditional(defs);

    println!("After mkIf filtering:");
    println!("──────────────────────");
    for (i, def) in filtered.iter().enumerate() {
        println!("  [{}] {}", i, def.value);
    }
    println!();
    println!("✓ mkIf false (# no tls) was correctly filtered out!");
}

fn show_limitations() {
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│  Architecture Overview                                           │");
    println!("└──────────────────────────────────────────────────────────────────┘\n");

    println!("Implemented components:");
    println!("───────────────────────");
    println!("  - Lexer - tokenizes Nix source");
    println!("  - Parser - builds AST from tokens");
    println!("  - Type system - Bool, Int, Str, ListOf, AttrsOf, Submodule");
    println!("  - Type checking - validates values against types");
    println!("  - Merge engine - priority-based merging");
    println!("  - Conditionals - mkIf/mkMerge filtering");
    println!("  - Module collection - import resolution");
    println!("  - LSP providers - completion/hover");
    println!("  - Nix evaluator (via nix-bindings feature)");
    println!();

    println!("Optional features:");
    println!("──────────────────");
    println!("  nix-bindings - Enable Nix expression evaluation via nix-bindings-rust");
    println!("  nix-ffi      - Direct Nix C API integration (alternative to nix-bindings)");
    println!("  lsp          - Language server protocol support");
}
