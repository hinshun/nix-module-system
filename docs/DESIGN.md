# Nix Module System v2: High-Performance Design Document

## Executive Summary

This document describes the architecture for a modern Nix module system implementation that:
- **Eliminates fixed-point evaluation** in favor of lattice-based unification
- **Implements core primitives in Rust** for maximum performance
- **Provides beautiful error messages** via ariadne
- **Includes a Language Server Protocol** implementation
- **Auto-generates reference documentation** to Unison-quality standards

## 1. Problem Analysis

### 1.1 Why the Current NixOS Module System is Slow

Based on analysis of `/tmp/nixpkgs/lib/modules.nix`, the performance bottlenecks are:

1. **Fixed-Point Cycle** (lines 259-275): `config` and `options` are thunks passed to every module during collection, causing:
   - O(n²) or worse evaluation when modules reference `config`
   - Full evaluation triggered during import resolution
   - No parallelization possible

2. **Recursive Merging** (`mergeModules'` lines 782-954):
   - Depth-first traversal of entire option tree
   - O(N × depth) work minimum
   - Sequential processing

3. **Submodule Explosion** (`submoduleWith` in types.nix):
   - Each submodule creates a complete new `evalModules` call
   - N submodules = N additional full evaluations

4. **Measured Impact** (from nixpkgs#8152):
   - 356% slowdown from NixOS 14.12 to 19.03
   - Module count +118%, options +211%
   - Large configs: 10-60+ seconds evaluation

### 1.2 The Fixed-Point Problem Illustrated

```nix
# Current: Everything depends on everything
evalModules = { modules, ... }:
  let
    config = ... merged options with definitions ...;  # Thunk
    options = ... merged declarations ...;              # Thunk

    # Every module receives both, creating potential cycles
    collected = collectModules {
      inherit config options;  # Passed to every module!
    };
  in ...;
```

The error message at line 270 reveals it:
> "if you get an infinite recursion here, you probably reference `config` in `imports`"

## 2. Architecture Overview

### 2.1 Core Principles

1. **No Fixed Points**: Use topological sorting + lattice unification
2. **Rust Primitives**: Types, merging, and evaluation in Rust
3. **Staged Evaluation**: Parse → Collect → Sort → Merge → Emit
4. **Parallel by Default**: Independent operations run concurrently

### 2.2 Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     Nix Evaluator                            │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                 Rust Plugin (libmodule.so)              │ │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐  │ │
│  │  │  Types   │  │  Merge   │  │ evalMods │  │ Errors  │  │ │
│  │  │ (attrsOf │  │  Engine  │  │  (DAG    │  │(ariadne)│  │ │
│  │  │  listOf  │  │          │  │  based)  │  │         │  │ │
│  │  │  etc)    │  │          │  │          │  │         │  │ │
│  │  └──────────┘  └──────────┘  └──────────┘  └─────────┘  │ │
│  └─────────────────────────────────────────────────────────┘ │
│                              ↕ FFI                           │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                    Nix Library                           │ │
│  │              (types.nix, options.nix)                    │ │
│  └─────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              ↕
┌─────────────────────────────────────────────────────────────┐
│                    External Tools                            │
│  ┌──────────────┐  ┌───────────────┐  ┌──────────────────┐  │
│  │     LSP      │  │  Doc Generator │  │   CLI Tools     │  │
│  │ (async-lsp)  │  │   (Unison-     │  │                 │  │
│  │              │  │    quality)    │  │                 │  │
│  └──────────────┘  └───────────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## 3. Detailed Design

### 3.1 Progressive Refinement Architecture

Instead of fixed-point iteration, we use a **staged pipeline**:

```
Stage 1: Parse
  ├─ Parallel parse all module files
  ├─ Extract: imports, options, config
  └─ Output: Module ASTs + dependency graph

Stage 2: Collect
  ├─ Resolve imports (topological sort)
  ├─ Build dependency DAG
  └─ Output: Ordered module list

Stage 3: Declare
  ├─ Process option declarations (no config access)
  ├─ Merge type definitions
  └─ Output: Complete option schema

Stage 4: Define
  ├─ Process config definitions
  ├─ Lattice-based value unification
  └─ Output: Final configuration
```

### 3.2 Lattice-Based Unification (Inspired by CUE)

Instead of iterative fixed-point:

```rust
/// Unification is the "meet" operation in our lattice
pub trait Unify {
    /// Combine two values. Returns None if incompatible.
    fn unify(&self, other: &Self) -> Option<Self>;
}

/// Core property: unification is commutative and associative
/// unify(a, b) == unify(b, a)
/// unify(unify(a, b), c) == unify(a, unify(b, c))

impl Unify for ConfigValue {
    fn unify(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            // Same values unify to themselves
            (ConfigValue::Bool(a), ConfigValue::Bool(b)) if a == b => Some(self.clone()),

            // Lists concatenate
            (ConfigValue::List(a), ConfigValue::List(b)) => {
                Some(ConfigValue::List([a.clone(), b.clone()].concat()))
            }

            // Attrs merge recursively
            (ConfigValue::Attrs(a), ConfigValue::Attrs(b)) => {
                Some(ConfigValue::Attrs(merge_attrs(a, b)?))
            }

            // Incompatible values fail
            _ => None,
        }
    }
}
```

**Benefits**:
- Order-independent: modules can be processed in parallel
- Single-pass: no iteration needed
- Early failure: conflicts detected immediately

### 3.3 Type System in Rust

```rust
/// Core type trait - all module types implement this
pub trait NixType: Send + Sync {
    /// Type name for error messages
    fn name(&self) -> &str;

    /// Check if a value matches this type
    fn check(&self, value: &NixValue) -> TypeResult;

    /// Merge multiple definitions
    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> MergeResult;

    /// Get nested type info for documentation
    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>>;
}

/// Built-in types implemented in Rust for performance
pub mod types {
    pub struct Str;
    pub struct Bool;
    pub struct Int;
    pub struct Path;
    pub struct ListOf { pub elem: Box<dyn NixType> }
    pub struct AttrsOf { pub elem: Box<dyn NixType> }
    pub struct Submodule { pub modules: Vec<Module> }
    pub struct OneOf { pub variants: Vec<Box<dyn NixType>> }
    pub struct Enum { pub values: Vec<String> }
    pub struct NullOr { pub inner: Box<dyn NixType> }
}
```

### 3.4 Rust Plugin Architecture

Based on nix-doc and the Nix C API:

```rust
// src/lib.rs - Plugin entry point

use std::ffi::{c_char, c_void, CStr};

/// Nix C API types
#[repr(C)]
pub struct EvalState { _private: [u8; 0] }

#[repr(C)]
pub struct Value { _private: [u8; 0] }

#[repr(C)]
pub struct NixContext { _private: [u8; 0] }

/// Our primop implementations
mod primops {
    pub mod types;      // Type checking primops
    pub mod merge;      // Merge engine
    pub mod eval;       // evalModules implementation
}

/// FFI exports called from C++
#[no_mangle]
pub unsafe extern "C" fn nms_check_type(
    ctx: *mut NixContext,
    state: *mut EvalState,
    type_name: *const c_char,
    value: *mut Value,
) -> bool {
    std::panic::catch_unwind(|| {
        let type_name = CStr::from_ptr(type_name).to_str().unwrap();
        primops::types::check(state, type_name, value)
    }).unwrap_or(false)
}

#[no_mangle]
pub unsafe extern "C" fn nms_merge_definitions(
    ctx: *mut NixContext,
    state: *mut EvalState,
    type_ptr: *mut c_void,
    defs: *mut Value,
    result: *mut Value,
) -> i32 {
    std::panic::catch_unwind(|| {
        primops::merge::merge_definitions(ctx, state, type_ptr, defs, result)
    }).unwrap_or(-1)
}
```

### 3.5 Error Reporting with Ariadne

```rust
use ariadne::{Color, ColorGenerator, Label, Report, ReportKind, Source};

pub struct ModuleError {
    pub kind: ErrorKind,
    pub primary: Span,
    pub secondary: Vec<(Span, String)>,
    pub message: String,
    pub notes: Vec<String>,
}

impl ModuleError {
    pub fn render(&self, cache: &impl ariadne::Cache<String>) -> String {
        let mut colors = ColorGenerator::new();
        let primary_color = colors.next();

        let mut report = Report::build(
            ReportKind::Error,
            self.primary.file.clone(),
            self.primary.start,
        )
        .with_code(self.kind.code())
        .with_message(&self.message)
        .with_label(
            Label::new((self.primary.file.clone(), self.primary.range()))
                .with_message(&self.kind.primary_message())
                .with_color(primary_color),
        );

        for (span, msg) in &self.secondary {
            report = report.with_label(
                Label::new((span.file.clone(), span.range()))
                    .with_message(msg)
                    .with_color(colors.next()),
            );
        }

        for note in &self.notes {
            report = report.with_note(note);
        }

        let mut output = Vec::new();
        report.finish().write(cache, &mut output).unwrap();
        String::from_utf8(output).unwrap()
    }
}

// Example output:
// Error[E0102]: Type mismatch in option definition
//    ┌─ /etc/nixos/configuration.nix:42:5
//    │
// 41 │   services.nginx = {
// 42 │     enable = "yes";
//    │     ^^^^^^^^^^^^^^ expected bool, found string
//    │
//    ├─ /nix/store/.../nixos/modules/services/nginx.nix:15:3
//    │
// 15 │     type = types.bool;
//    │     ------------------ option declared here as bool
//    │
//    = help: Use `true` or `false` instead of "yes"
```

### 3.6 LSP Architecture

Using async-lsp (same author as nil):

```rust
use async_lsp::{LanguageServer, MainLoop};
use lsp_types::*;

pub struct NixModuleLsp {
    documents: DashMap<Url, DocumentState>,
    options_index: OptionsIndex,
    diagnostics: DiagnosticsEngine,
}

#[async_trait]
impl LanguageServer for NixModuleLsp {
    async fn initialize(&mut self, params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), "=".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn completion(
        &mut self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Get option path at cursor
        let doc = self.documents.get(uri)?;
        let path = doc.get_option_path_at(position);

        // Complete from options index
        let completions = self.options_index
            .complete_path(&path)
            .into_iter()
            .map(|opt| CompletionItem {
                label: opt.name.clone(),
                kind: Some(CompletionItemKind::PROPERTY),
                documentation: opt.description.map(|d| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: d,
                    })
                }),
                ..Default::default()
            })
            .collect();

        Ok(Some(CompletionResponse::Array(completions)))
    }
}
```

## 4. Data Structures

### 4.1 Module Representation

```rust
/// Parsed module before evaluation
pub struct Module {
    pub file: PathBuf,
    pub key: String,
    pub imports: Vec<Import>,
    pub options: OptionTree,
    pub config: ConfigTree,
    pub meta: ModuleMeta,
}

/// Import can be path, function, or inline module
pub enum Import {
    Path(PathBuf),
    Function { file: PathBuf, args: Vec<String> },
    Inline(Box<Module>),
}

/// Hierarchical option declarations
pub struct OptionTree {
    pub children: HashMap<String, OptionNode>,
}

pub enum OptionNode {
    Option(OptionDecl),
    Nested(OptionTree),
}

pub struct OptionDecl {
    pub type_: Box<dyn NixType>,
    pub default: Option<NixValue>,
    pub description: Option<String>,
    pub example: Option<NixValue>,
    pub visible: bool,
    pub read_only: bool,
    pub location: Span,
}
```

### 4.2 Evaluation State

```rust
/// State during module evaluation (no fixed-point!)
pub struct EvalContext {
    /// Topologically sorted modules
    pub modules: Vec<Module>,

    /// Merged option schema (after Stage 3)
    pub schema: OptionSchema,

    /// In-progress config definitions
    pub definitions: HashMap<OptionPath, Vec<Definition>>,

    /// Error accumulator
    pub errors: Vec<ModuleError>,

    /// Source cache for error reporting
    pub sources: SourceCache,
}

pub struct Definition {
    pub file: PathBuf,
    pub location: Span,
    pub value: NixValue,
    pub priority: i32,  // mkOverride priority
    pub condition: Option<NixValue>,  // mkIf condition
}
```

## 5. Implementation Plan

### Phase 1: Core Types in Rust (Week 1-2)
- [ ] Set up Rust plugin skeleton with C++ FFI
- [ ] Implement basic types (str, bool, int, path)
- [ ] Implement compound types (listOf, attrsOf)
- [ ] Implement type checking primop
- [ ] Add ariadne error formatting

### Phase 2: Merge Engine (Week 3-4)
- [ ] Implement merge strategies per type
- [ ] Implement priority system (mkDefault, mkForce)
- [ ] Implement conditional merging (mkIf, mkMerge)
- [ ] Parallel merge for independent paths

### Phase 3: evalModules (Week 5-6)
- [ ] Implement module parsing and collection
- [ ] Implement topological sorting
- [ ] Implement staged evaluation pipeline
- [ ] Remove fixed-point dependency

### Phase 4: LSP Server (Week 7-8)
- [ ] Set up async-lsp framework
- [ ] Implement document synchronization
- [ ] Implement option completion
- [ ] Implement go-to-definition
- [ ] Implement hover documentation

### Phase 5: Documentation Generator (Week 9-10)
- [ ] Design documentation schema
- [ ] Implement option extraction
- [ ] Implement Markdown/HTML rendering
- [ ] Add cross-reference support

## 6. Testing Strategy

### 6.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_str_valid() {
        let ty = types::Str;
        assert!(ty.check(&NixValue::String("hello".into())).is_ok());
    }

    #[test]
    fn test_type_str_invalid() {
        let ty = types::Str;
        let result = ty.check(&NixValue::Int(42));
        assert!(matches!(result, Err(TypeError::Mismatch { .. })));
    }

    #[test]
    fn test_merge_list_concat() {
        let ty = types::ListOf { elem: Box::new(types::Str) };
        let defs = vec![
            Definition::new(vec!["a".into(), "b".into()]),
            Definition::new(vec!["c".into()]),
        ];
        let result = ty.merge(&OptionPath::root(), defs).unwrap();
        assert_eq!(result, NixValue::List(vec!["a", "b", "c"]));
    }
}
```

### 6.2 Integration Tests

```nix
# tests/basic.nix
{ lib, ... }:
{
  options.test.value = lib.mkOption {
    type = lib.types.str;
    default = "hello";
  };

  config.test.value = "world";
}

# Expected: config.test.value == "world"
```

### 6.3 Smoke Tests

```bash
#!/bin/bash
# tests/smoke.sh

# Build the plugin
cargo build --release

# Test basic evaluation
nix eval --plugin-files ./target/release/libnix_module_system.so \
  --expr 'builtins.nms_typeCheck "str" "hello"'
# Expected: true

# Test type error
nix eval --plugin-files ./target/release/libnix_module_system.so \
  --expr 'builtins.nms_typeCheck "int" "hello"'
# Expected: false (with nice error message)
```

## 7. File Structure

```
nix-module-system/
├── Cargo.toml
├── build.rs                  # C++ compilation
├── flake.nix                 # Nix build
├── src/
│   ├── lib.rs               # Plugin entry point
│   ├── ffi/
│   │   ├── mod.rs
│   │   ├── plugin.cpp       # C++ Nix interface
│   │   └── compat.h         # Nix version compat
│   ├── types/
│   │   ├── mod.rs
│   │   ├── base.rs          # str, bool, int, etc.
│   │   ├── compound.rs      # listOf, attrsOf
│   │   ├── submodule.rs
│   │   └── trait.rs         # NixType trait
│   ├── merge/
│   │   ├── mod.rs
│   │   ├── strategy.rs      # Merge strategies
│   │   ├── priority.rs      # mkOverride handling
│   │   └── lattice.rs       # Unification
│   ├── eval/
│   │   ├── mod.rs
│   │   ├── collect.rs       # Module collection
│   │   ├── topo.rs          # Topological sort
│   │   └── pipeline.rs      # Staged evaluation
│   ├── errors/
│   │   ├── mod.rs
│   │   ├── types.rs         # Error kinds
│   │   └── render.rs        # Ariadne rendering
│   └── lsp/
│       ├── mod.rs
│       ├── server.rs
│       ├── completion.rs
│       └── hover.rs
├── nix/
│   ├── lib.nix              # Nix-side library
│   ├── types.nix            # Type wrappers
│   └── options.nix          # Option helpers
├── tests/
│   ├── types.rs
│   ├── merge.rs
│   ├── eval.rs
│   └── fixtures/
└── docs/
    ├── DESIGN.md            # This document
    └── API.md               # API reference
```

## 8. Performance Targets

| Metric | Current NixOS | Target |
|--------|--------------|--------|
| Small config (<100 options) | 0.5s | <50ms |
| Medium config (100-1000) | 5s | <200ms |
| Large config (1000+) | 30-60s | <2s |
| Incremental update | N/A | <50ms |
| LSP completion | N/A | <100ms |

## 9. Compatibility

### 9.1 Nix Version Support
- Minimum: Nix 2.18 (C API stability improvements)
- Target: Nix 2.20+

### 9.2 NixOS Module Compatibility
- Goal: Drop-in replacement for existing modules
- Provide shim layer for gradual migration
- Warn on patterns requiring fixed-point

## 10. Open Questions

1. **Submodule Evaluation**: Should submodules use separate evaluation contexts or share parent context?

2. **Circular Imports**: How to handle intentional circular references that work with lazy evaluation?

3. **Caching Strategy**: How to persist evaluation cache across builds?

4. **Debug Mode**: How to provide detailed traces for debugging module issues?

---

## References

- [NixOS Module System Source](https://github.com/NixOS/nixpkgs/blob/master/lib/modules.nix)
- [CUE Language Unification](https://cuelang.org/docs/concept/the-logic-of-cue/)
- [Determinate Systems Parallel Evaluation](https://determinate.systems/blog/parallel-nix-eval/)
- [nix-doc Plugin](https://github.com/lf-/nix-doc)
- [Nix C API](https://nix.dev/manual/nix/2.24/c-api)
- [Ariadne Error Reporting](https://docs.rs/ariadne)
- [async-lsp Framework](https://github.com/oxalica/async-lsp)
