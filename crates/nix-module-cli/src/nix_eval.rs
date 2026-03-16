//! Real Nix evaluator using nix-bindings-rust.
//!
//! This module provides a full `NixEvaluator` that can evaluate Nix expressions
//! by embedding a Nix `EvalState` in the Rust process.

use nix_module_system::nix::error::{NixError, NixResult, TraceFrame};
use nix_module_system::nix::NixConfig;
use nix_module_system::types::Value;

use nix_bindings_expr::eval_state::{gc_register_my_thread, init, EvalState, ThreadRegistrationGuard};
use nix_bindings_expr::value::Value as NixBindingsValue;
use nix_bindings_store::store::Store;

use indexmap::IndexMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Global initialization flag.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the Nix library.
///
/// This must be called once before creating any evaluators.
/// It's safe to call multiple times.
pub fn initialize() -> NixResult<()> {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    init().map_err(|e| NixError::InitError {
        message: e.to_string(),
    })
}

/// High-level Nix evaluator backed by nix-bindings-rust.
pub struct NixEvaluator {
    /// The evaluation state.
    eval_state: EvalState,
    /// Thread registration guard.
    _guard: ThreadRegistrationGuard,
    /// Configuration.
    config: NixConfig,
}

impl NixEvaluator {
    /// Create a new evaluator with the given configuration.
    pub fn new(config: NixConfig) -> NixResult<Self> {
        initialize()?;

        let guard = gc_register_my_thread().map_err(|e| NixError::ThreadRegistrationError {
            message: e.to_string(),
        })?;

        let store = Store::open(config.store_uri.as_deref(), config.store_params.clone())
            .map_err(|e| NixError::StoreError {
                message: e.to_string(),
            })?;

        let eval_state = EvalState::new(store, config.lookup_paths.clone()).map_err(|e| {
            NixError::InitError {
                message: e.to_string(),
            }
        })?;

        Ok(Self {
            eval_state,
            _guard: guard,
            config,
        })
    }

    /// Evaluate a Nix expression from a string.
    pub fn evaluate_expr(&mut self, expr: &str) -> NixResult<Value> {
        self.evaluate_expr_with_source(expr, "<expr>")
    }

    /// Evaluate a Nix expression with a custom source name.
    pub fn evaluate_expr_with_source(
        &mut self,
        expr: &str,
        source_name: &str,
    ) -> NixResult<Value> {
        let nix_value = self
            .eval_state
            .eval_from_string(expr, source_name)
            .map_err(|e| parse_nix_error(e, source_name))?;

        nix_to_value(&mut self.eval_state, &nix_value)
    }

    /// Evaluate a Nix file.
    pub fn evaluate_file(&mut self, path: impl AsRef<Path>) -> NixResult<Value> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(NixError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        let contents = std::fs::read_to_string(path).map_err(|e| NixError::IoError {
            message: format!("failed to read {}: {}", path.display(), e),
        })?;

        let source_name = path.to_string_lossy();
        self.evaluate_expr_with_source(&contents, &source_name)
    }

    /// Evaluate a Nix expression and return a lazy value.
    pub fn evaluate_lazy(&mut self, expr: &str) -> NixResult<LazyValue> {
        let nix_value = self
            .eval_state
            .eval_from_string(expr, "<expr>")
            .map_err(|e| parse_nix_error(e, "<expr>"))?;

        Ok(LazyValue::new(nix_value))
    }

    /// Evaluate a Nix expression and extract an attribute.
    pub fn evaluate_attr(&mut self, expr: &str, attr: &str) -> NixResult<Value> {
        let full_expr = format!("({}).{}", expr, attr);
        self.evaluate_expr(&full_expr)
    }

    /// Call a Nix function with an argument.
    pub fn call_function(&mut self, func_expr: &str, arg: &Value) -> NixResult<Value> {
        let func_value = self
            .eval_state
            .eval_from_string(func_expr, "<function>")
            .map_err(|e| parse_nix_error(e, "<function>"))?;

        let arg_value = value_to_nix(&mut self.eval_state, arg)?;

        let result = self
            .eval_state
            .call(func_value, arg_value)
            .map_err(|e| NixError::evaluation(e.to_string()))?;

        nix_to_value(&mut self.eval_state, &result)
    }

    /// Import a Nix file and return its value.
    pub fn import(&mut self, path: impl AsRef<Path>) -> NixResult<Value> {
        let path = path.as_ref();
        let expr = format!("import {}", path.display());
        self.evaluate_expr(&expr)
    }

    /// Evaluate with builtins available in scope.
    pub fn evaluate_with_builtins(&mut self, expr: &str) -> NixResult<Value> {
        self.evaluate_expr(expr)
    }

    /// Get the raw EvalState for advanced operations.
    pub fn eval_state(&mut self) -> &mut EvalState {
        &mut self.eval_state
    }

    /// Get the configuration.
    pub fn config(&self) -> &NixConfig {
        &self.config
    }
}

/// Convert a Nix value to our internal Value type.
pub fn nix_to_value(state: &mut EvalState, nix_val: &NixBindingsValue) -> NixResult<Value> {
    use nix_bindings_expr::value::ValueType;

    let val_type = state.value_type(nix_val);

    match val_type {
        ValueType::Null => Ok(Value::Null),

        ValueType::Bool => {
            let b = state
                .require_bool(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Bool(b))
        }

        ValueType::Int => {
            let i = state
                .require_int(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Int(i))
        }

        ValueType::Float => {
            let f = state
                .require_float(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Float(f))
        }

        ValueType::String => {
            let s = state
                .require_string(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::String(s))
        }

        ValueType::Path => {
            let p = state
                .require_path(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            Ok(Value::Path(PathBuf::from(p)))
        }

        ValueType::List => {
            let size = state
                .require_list_size(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;

            let mut elements = Vec::with_capacity(size as usize);
            for i in 0..size {
                if let Some(elem) = state
                    .require_list_select_idx_strict(nix_val, i)
                    .map_err(|e| NixError::evaluation(e.to_string()))?
                {
                    elements.push(nix_to_value(state, &elem)?);
                }
            }
            Ok(Value::List(elements))
        }

        ValueType::Attrs => {
            // Check if this is a derivation
            if let Ok(Some(type_val)) = state.require_attrs_select_opt(nix_val, "type") {
                if let Ok(type_str) = state.require_string(&type_val) {
                    if type_str == "derivation" {
                        let inner = nix_attrs_to_value(state, nix_val)?;
                        return Ok(Value::Derivation(Box::new(inner)));
                    }
                }
            }

            nix_attrs_to_value(state, nix_val)
        }

        ValueType::Lambda | ValueType::PrimOp | ValueType::PrimOpApp => Ok(Value::Lambda),

        ValueType::Thunk => {
            state
                .force(nix_val)
                .map_err(|e| NixError::evaluation(e.to_string()))?;
            nix_to_value(state, nix_val)
        }

        ValueType::External => {
            Err(NixError::conversion("cannot convert external value"))
        }
    }
}

/// Convert Nix attrs to our Value::Attrs.
fn nix_attrs_to_value(state: &mut EvalState, nix_val: &NixBindingsValue) -> NixResult<Value> {
    let names = state
        .require_attrs_names(nix_val)
        .map_err(|e| NixError::evaluation(e.to_string()))?;

    let mut attrs = IndexMap::with_capacity(names.len());
    for name in names {
        let attr_val = state
            .require_attrs_select(nix_val, &name)
            .map_err(|e| NixError::evaluation(e.to_string()))?;
        attrs.insert(name, nix_to_value(state, &attr_val)?);
    }

    Ok(Value::Attrs(attrs))
}

/// Convert our Value type to a Nix value.
pub fn value_to_nix(state: &mut EvalState, value: &Value) -> NixResult<NixBindingsValue> {
    match value {
        Value::Null => state
            .new_value_null()
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Bool(b) => state
            .new_value_bool(*b)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Int(i) => state
            .new_value_int(*i)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Float(f) => state
            .new_value_float(*f)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::String(s) => state
            .new_value_str(s)
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::Path(p) => state
            .new_value_path(&p.to_string_lossy())
            .map_err(|e| NixError::evaluation(e.to_string())),

        Value::List(items) => {
            let mut nix_items = Vec::with_capacity(items.len());
            for item in items {
                nix_items.push(value_to_nix(state, item)?);
            }
            state
                .new_value_list(nix_items)
                .map_err(|e| NixError::evaluation(e.to_string()))
        }

        Value::Attrs(attrs) => {
            let mut nix_attrs = Vec::with_capacity(attrs.len());
            for (k, v) in attrs {
                nix_attrs.push((k.clone(), value_to_nix(state, v)?));
            }
            state
                .new_value_attrs(nix_attrs)
                .map_err(|e| NixError::evaluation(e.to_string()))
        }

        Value::Lambda => {
            Err(NixError::conversion(
                "cannot convert lambda to Nix value without context",
            ))
        }

        Value::Derivation(inner) => {
            value_to_nix(state, inner)
        }
    }
}

/// Lazy value wrapper that defers conversion until needed.
pub struct LazyValue {
    nix_value: NixBindingsValue,
    cached: Option<Value>,
}

impl LazyValue {
    /// Create a new lazy value.
    pub fn new(nix_value: NixBindingsValue) -> Self {
        Self {
            nix_value,
            cached: None,
        }
    }

    /// Get the underlying Nix value.
    pub fn nix_value(&self) -> &NixBindingsValue {
        &self.nix_value
    }

    /// Force evaluation and convert to our Value type.
    pub fn force(&mut self, state: &mut EvalState) -> NixResult<&Value> {
        if self.cached.is_none() {
            let value = nix_to_value(state, &self.nix_value)?;
            self.cached = Some(value);
        }
        Ok(self.cached.as_ref().unwrap())
    }

    /// Take the converted value, consuming self.
    pub fn into_value(mut self, state: &mut EvalState) -> NixResult<Value> {
        self.force(state)?;
        Ok(self.cached.unwrap())
    }
}

/// Parse a nix-bindings error into our error type.
fn parse_nix_error(err: anyhow::Error, source: &str) -> NixError {
    let message = err.to_string();

    if let Some(trace) = extract_trace(&message) {
        return NixError::evaluation_with_trace(message.clone(), trace);
    }

    if message.contains("syntax error") || message.contains("unexpected") {
        return NixError::ParseError {
            file: PathBuf::from(source),
            message,
            line: None,
            column: None,
        };
    }

    if message.contains("undefined variable") {
        return NixError::evaluation(message);
    }

    if message.contains("attribute") && message.contains("missing") {
        return NixError::evaluation(message);
    }

    NixError::Other(err)
}

/// Extract trace frames from an error message.
fn extract_trace(message: &str) -> Option<Vec<TraceFrame>> {
    let mut frames = Vec::new();

    for line in message.lines() {
        if let Some(at_pos) = line.find(" at ") {
            let location = &line[at_pos + 4..];
            if let Some((file, rest)) = location.rsplit_once(':') {
                if let Some((line_col, _)) = rest.split_once(':') {
                    if let Ok(line_num) = line_col.parse::<usize>() {
                        let desc = line[..at_pos].trim().to_string();
                        frames.push(
                            TraceFrame::new(desc)
                                .with_location(PathBuf::from(file), line_num, 1),
                        );
                    }
                }
            }
        }
    }

    if frames.is_empty() {
        None
    } else {
        Some(frames)
    }
}
