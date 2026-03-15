//! Staged evaluation pipeline.
//!
//! The pipeline processes modules in four stages:
//! 1. **Parse**: Extract module structure (already done during collection)
//! 2. **Collect**: Resolve imports and build dependency graph
//! 3. **Declare**: Process option declarations to build schema
//! 4. **Define**: Merge config definitions using the type system

use super::{CollectedModule, EvalResult, OptionInfo};
use crate::errors::EvalError;
use crate::merge::process_conditional;
use crate::merge::MergeEngine;
use crate::parse::{AttrName, Binding, Expr, Spanned};
use crate::types::{Definition, OptionPath, Value};
use indexmap::IndexMap;
use std::path::PathBuf;

/// The evaluation pipeline stages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Parsing modules
    Parse,
    /// Collecting imports
    Collect,
    /// Declaring options
    Declare,
    /// Defining values
    Define,
    /// Complete
    Complete,
}

/// An option declaration extracted from a module
#[derive(Debug, Clone)]
pub struct OptionDeclaration {
    /// Where this option was declared
    pub file: PathBuf,
    /// The type expression (as AST)
    pub type_expr: Option<Spanned<Expr>>,
    /// Default value expression
    pub default_expr: Option<Spanned<Expr>>,
    /// Description
    pub description: Option<String>,
    /// Example
    pub example_expr: Option<Spanned<Expr>>,
    /// Whether internal
    pub internal: bool,
    /// Whether visible
    pub visible: bool,
    /// Whether read-only
    pub read_only: bool,
}

/// A config definition extracted from a module
#[derive(Debug, Clone)]
pub struct ConfigDefinition {
    /// Where this config was defined
    pub file: PathBuf,
    /// The value expression
    pub value_expr: Spanned<Expr>,
    /// Priority (from mkOverride, mkDefault, mkForce)
    pub priority: i32,
}

/// The evaluation pipeline
pub struct Pipeline {
    stage: Stage,
    modules: Vec<CollectedModule>,
    /// Option declarations by path
    declarations: IndexMap<OptionPath, Vec<OptionDeclaration>>,
    /// Config definitions by path
    definitions: IndexMap<OptionPath, Vec<ConfigDefinition>>,
    /// Merged option info
    options: IndexMap<OptionPath, OptionInfo>,
    /// Merge engine
    merge_engine: MergeEngine,
    /// Warnings accumulated during evaluation
    warnings: Vec<String>,
}

impl Pipeline {
    /// Create a new pipeline
    pub fn new() -> Self {
        Self {
            stage: Stage::Parse,
            modules: Vec::new(),
            declarations: IndexMap::new(),
            definitions: IndexMap::new(),
            options: IndexMap::new(),
            merge_engine: MergeEngine::new(),
            warnings: Vec::new(),
        }
    }

    /// Get current stage
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// Add modules to evaluate
    pub fn with_modules(mut self, modules: Vec<CollectedModule>) -> Self {
        self.modules = modules;
        self
    }

    /// Run the full pipeline
    pub fn run(mut self) -> Result<EvalResult, EvalError> {
        // Stage 1: Parse (already done during collection)
        self.stage = Stage::Collect;

        // Stage 2: Collect (modules are already collected)
        self.stage = Stage::Declare;

        // Stage 3: Declare options
        self.declare_options()?;
        self.stage = Stage::Define;

        // Stage 4: Define values
        let config = self.define_values()?;
        self.stage = Stage::Complete;

        Ok(EvalResult {
            config,
            options: self.options,
            warnings: self.warnings,
        })
    }

    /// Process option declarations from all modules
    fn declare_options(&mut self) -> Result<(), EvalError> {
        // Collect module data first to avoid borrow issues
        let module_data: Vec<_> = self
            .modules
            .iter()
            .filter(|m| !m.disabled)
            .filter_map(|m| m.options.as_ref().map(|o| (m.file.clone(), o.clone())))
            .collect();

        for (file, options_expr) in module_data {
            self.extract_options(&file, &options_expr, &OptionPath::root())?;
        }

        // Build merged option info from declarations
        self.build_option_schema()?;

        Ok(())
    }

    /// Extract option declarations from an options expression
    fn extract_options(
        &mut self,
        file: &PathBuf,
        expr: &Spanned<Expr>,
        prefix: &OptionPath,
    ) -> Result<(), EvalError> {
        match &expr.node {
            Expr::AttrSet(attrs) => {
                for binding in &attrs.bindings {
                    if let Binding::Simple { path, value } = binding {
                        let attr_path = extract_attr_path(path);
                        let full_path = prefix.extend(&attr_path);

                        // Check if this is an mkOption call
                        if is_mk_option(value) {
                            let decl = extract_option_declaration(file, value);
                            self.declarations
                                .entry(full_path)
                                .or_default()
                                .push(decl);
                        } else {
                            // Nested options
                            self.extract_options(file, value, &full_path)?;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Build the merged option schema from declarations
    fn build_option_schema(&mut self) -> Result<(), EvalError> {
        for (path, decls) in &self.declarations {
            let mut declared_in = Vec::new();
            let mut type_desc = "unspecified".to_string();
            let mut default = None;
            let mut description = None;

            for decl in decls {
                declared_in.push(decl.file.clone());

                // Use first non-None values
                if decl.type_expr.is_some() && type_desc == "unspecified" {
                    type_desc = format_type_expr(decl.type_expr.as_ref());
                }
                if default.is_none() {
                    default = decl.default_expr.as_ref().map(|e| expr_to_value(e));
                }
                if description.is_none() {
                    description = decl.description.clone();
                }
            }

            self.options.insert(
                path.clone(),
                OptionInfo {
                    path: path.clone(),
                    type_desc,
                    default,
                    description,
                    declared_in,
                    internal: false,
                },
            );
        }

        Ok(())
    }

    /// Extract config definitions from all modules
    fn extract_definitions(&mut self) -> Result<(), EvalError> {
        // Collect module data first to avoid borrow issues
        let module_data: Vec<_> = self
            .modules
            .iter()
            .filter(|m| !m.disabled)
            .filter_map(|m| m.config.as_ref().map(|c| (m.file.clone(), c.clone())))
            .collect();

        for (file, config_expr) in module_data {
            self.extract_config(&file, &config_expr, &OptionPath::root())?;
        }

        Ok(())
    }

    /// Extract config definitions from a config expression
    fn extract_config(
        &mut self,
        file: &PathBuf,
        expr: &Spanned<Expr>,
        prefix: &OptionPath,
    ) -> Result<(), EvalError> {
        match &expr.node {
            Expr::AttrSet(attrs) => {
                for binding in &attrs.bindings {
                    if let Binding::Simple { path, value } = binding {
                        let attr_path = extract_attr_path(path);
                        let full_path = prefix.extend(&attr_path);

                        // Extract priority from mkDefault/mkForce/mkOverride
                        let (priority, inner_value) = extract_priority(value);

                        self.definitions
                            .entry(full_path)
                            .or_default()
                            .push(ConfigDefinition {
                                file: file.clone(),
                                value_expr: inner_value.clone(),
                                priority,
                            });
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Merge config definitions to produce final config
    fn define_values(&mut self) -> Result<Value, EvalError> {
        // First extract all definitions
        self.extract_definitions()?;

        // Merge definitions for each option
        let mut result = IndexMap::new();

        for (path, defs) in &self.definitions {
            // Convert to Definition structs with evaluated values
            let mut typed_defs: Vec<Definition> = defs
                .iter()
                .map(|d| {
                    let mut def = Definition::new(expr_to_value(&d.value_expr));
                    def.file = d.file.clone();
                    def.priority = d.priority;
                    def
                })
                .collect();

            // Process conditionals (mkIf, mkMerge)
            typed_defs = typed_defs
                .into_iter()
                .flat_map(|def| {
                    let processed = process_conditional(def.value.clone());
                    processed.into_iter().map(move |v| {
                        let mut new_def = def.clone();
                        new_def.value = v;
                        new_def
                    })
                })
                .collect();

            if typed_defs.is_empty() {
                continue;
            }

            // Sort by priority (lower priority wins)
            typed_defs.sort_by_key(|d| d.priority);

            // If all definitions have the same priority, we need to merge
            // If different priorities, take the lowest
            let min_priority = typed_defs[0].priority;
            let same_priority_defs: Vec<_> = typed_defs
                .into_iter()
                .filter(|d| d.priority == min_priority)
                .collect();

            // Merge definitions
            let merged_value = if same_priority_defs.len() == 1 {
                same_priority_defs[0].value.clone()
            } else {
                // Use merge engine for multiple definitions
                let option_info = self.options.get(path);
                self.merge_values(&same_priority_defs, option_info)?
            };

            // Insert into result at the correct path
            insert_at_path(&mut result, path, merged_value);
        }

        // Apply defaults for options without definitions
        for (path, info) in &self.options {
            if !has_value_at_path(&result, path) {
                if let Some(ref default) = info.default {
                    insert_at_path(&mut result, path, default.clone());
                }
            }
        }

        Ok(Value::Attrs(result))
    }

    /// Merge multiple definitions for the same option
    fn merge_values(
        &self,
        defs: &[Definition],
        option_info: Option<&OptionInfo>,
    ) -> Result<Value, EvalError> {
        if defs.is_empty() {
            return Ok(Value::Null);
        }

        if defs.len() == 1 {
            return Ok(defs[0].value.clone());
        }

        // Determine merge strategy based on type
        let type_desc = option_info
            .map(|o| o.type_desc.as_str())
            .unwrap_or("unspecified");

        match type_desc {
            t if t.starts_with("listOf") => {
                // Concatenate lists
                let mut result = Vec::new();
                for def in defs {
                    if let Value::List(items) = &def.value {
                        result.extend(items.iter().cloned());
                    }
                }
                Ok(Value::List(result))
            }
            t if t.starts_with("attrsOf") => {
                // Merge attribute sets
                let mut result = IndexMap::new();
                for def in defs {
                    if let Value::Attrs(attrs) = &def.value {
                        for (k, v) in attrs {
                            result.insert(k.clone(), v.clone());
                        }
                    }
                }
                Ok(Value::Attrs(result))
            }
            _ => {
                // For other types, last wins (or error on conflict)
                Ok(defs.last().unwrap().value.clone())
            }
        }
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract attribute path from binding path
fn extract_attr_path(path: &[Spanned<AttrName>]) -> Vec<String> {
    path.iter()
        .filter_map(|spanned| match &spanned.node {
            AttrName::Ident(s) => Some(s.clone()),
            AttrName::String(parts) => parts.as_simple().map(|s| s.to_string()),
            AttrName::Interpolation(_) => None,
        })
        .collect()
}

/// Check if an expression is an mkOption call
fn is_mk_option(expr: &Spanned<Expr>) -> bool {
    match &expr.node {
        Expr::Apply { func, .. } => match &func.node {
            Expr::Ident(name) => name == "mkOption",
            Expr::Select { expr: base, path, .. } => {
                // lib.mkOption
                if let Expr::Ident(base_name) = &base.node {
                    if base_name == "lib" && path.len() == 1 {
                        if let AttrName::Ident(attr) = &path[0].node {
                            return attr == "mkOption";
                        }
                    }
                }
                false
            }
            _ => false,
        },
        _ => false,
    }
}

/// Extract option declaration from mkOption call
fn extract_option_declaration(file: &PathBuf, expr: &Spanned<Expr>) -> OptionDeclaration {
    let mut decl = OptionDeclaration {
        file: file.clone(),
        type_expr: None,
        default_expr: None,
        description: None,
        example_expr: None,
        internal: false,
        visible: true,
        read_only: false,
    };

    if let Expr::Apply { arg, .. } = &expr.node {
        if let Expr::AttrSet(attrs) = &arg.node {
            for binding in &attrs.bindings {
                if let Binding::Simple { path, value } = binding {
                    if let Some(name) = path.first() {
                        if let AttrName::Ident(attr_name) = &name.node {
                            match attr_name.as_str() {
                                "type" => decl.type_expr = Some(value.clone()),
                                "default" => decl.default_expr = Some(value.clone()),
                                "description" => {
                                    if let Expr::String(parts) = &value.node {
                                        decl.description = parts.as_simple().map(|s| s.to_string());
                                    }
                                }
                                "example" => decl.example_expr = Some(value.clone()),
                                "internal" => {
                                    if let Expr::Bool(b) = &value.node {
                                        decl.internal = *b;
                                    }
                                }
                                "visible" => {
                                    if let Expr::Bool(b) = &value.node {
                                        decl.visible = *b;
                                    }
                                }
                                "readOnly" => {
                                    if let Expr::Bool(b) = &value.node {
                                        decl.read_only = *b;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    decl
}

/// Format a type expression for display
fn format_type_expr(expr: Option<&Spanned<Expr>>) -> String {
    match expr {
        None => "unspecified".to_string(),
        Some(e) => match &e.node {
            Expr::Ident(name) => name.clone(),
            Expr::Select { expr: base, path, .. } => {
                let base_str = format_type_expr(Some(base));
                let path_str = path
                    .iter()
                    .filter_map(|p| match &p.node {
                        AttrName::Ident(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                format!("{}.{}", base_str, path_str)
            }
            Expr::Apply { func, arg } => {
                let func_str = format_type_expr(Some(func));
                let arg_str = format_type_expr(Some(arg));
                format!("{} {}", func_str, arg_str)
            }
            _ => "unknown".to_string(),
        },
    }
}

/// Extract priority from mkDefault/mkForce/mkOverride
fn extract_priority(expr: &Spanned<Expr>) -> (i32, &Spanned<Expr>) {
    match &expr.node {
        Expr::Apply { func, arg } => match &func.node {
            Expr::Ident(name) => match name.as_str() {
                "mkDefault" => (1000, arg.as_ref()),
                "mkForce" => (50, arg.as_ref()),
                _ => (100, expr),
            },
            Expr::Apply { func: inner_func, arg: priority_arg } => {
                // mkOverride priority value
                if let Expr::Ident(name) = &inner_func.node {
                    if name == "mkOverride" {
                        if let Expr::Int(p) = &priority_arg.node {
                            return (*p as i32, arg.as_ref());
                        }
                    }
                }
                (100, expr)
            }
            _ => (100, expr),
        },
        _ => (100, expr),
    }
}

/// Convert an AST expression to a Value
fn expr_to_value(expr: &Spanned<Expr>) -> Value {
    match &expr.node {
        Expr::Null => Value::Null,
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Int(i) => Value::Int(*i),
        Expr::Float(f) => Value::Float(*f),
        Expr::String(parts) => {
            if let Some(s) = parts.as_simple() {
                Value::String(s.to_string())
            } else {
                // Interpolated strings need evaluation
                Value::String("<interpolated>".to_string())
            }
        }
        Expr::Path(p) => Value::Path(p.clone()),
        Expr::List(items) => Value::List(items.iter().map(expr_to_value).collect()),
        Expr::AttrSet(attrs) => {
            let mut result = IndexMap::new();
            for binding in &attrs.bindings {
                if let Binding::Simple { path, value } = binding {
                    if let Some(name) = path.first() {
                        if let AttrName::Ident(key) = &name.node {
                            result.insert(key.clone(), expr_to_value(value));
                        }
                    }
                }
            }
            Value::Attrs(result)
        }
        Expr::Lambda(_) => Value::Lambda,
        _ => Value::Null, // Other expressions need evaluation
    }
}

/// Insert a value at a path in an attrs map
fn insert_at_path(attrs: &mut IndexMap<String, Value>, path: &OptionPath, value: Value) {
    let components = path.components();
    if components.is_empty() {
        return;
    }

    if components.len() == 1 {
        attrs.insert(components[0].clone(), value);
        return;
    }

    // Navigate/create nested attrs
    let first = &components[0];
    let rest = OptionPath::new(components[1..].to_vec());

    let nested = attrs
        .entry(first.clone())
        .or_insert_with(|| Value::Attrs(IndexMap::new()));

    if let Value::Attrs(nested_attrs) = nested {
        insert_at_path(nested_attrs, &rest, value);
    }
}

/// Check if a value exists at a path
fn has_value_at_path(attrs: &IndexMap<String, Value>, path: &OptionPath) -> bool {
    let components = path.components();
    if components.is_empty() {
        return false;
    }

    if components.len() == 1 {
        return attrs.contains_key(&components[0]);
    }

    let first = &components[0];
    let rest = OptionPath::new(components[1..].to_vec());

    match attrs.get(first) {
        Some(Value::Attrs(nested)) => has_value_at_path(nested, &rest),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_stages() {
        let pipeline = Pipeline::new();
        assert_eq!(pipeline.stage(), Stage::Parse);
    }

    #[test]
    fn test_empty_pipeline() {
        let result = Pipeline::new().run().unwrap();
        assert!(matches!(result.config, Value::Attrs(_)));
    }

    #[test]
    fn test_extract_attr_path() {
        use crate::parse::AttrName;
        use crate::types::Span;
        use std::path::PathBuf;

        let span = Span::new(PathBuf::from("test"), 0, 1, 1, 1);
        let path = vec![
            Spanned::new(AttrName::Ident("services".to_string()), span.clone()),
            Spanned::new(AttrName::Ident("nginx".to_string()), span),
        ];

        let result = extract_attr_path(&path);
        assert_eq!(result, vec!["services", "nginx"]);
    }

    #[test]
    fn test_insert_at_path() {
        let mut attrs = IndexMap::new();
        let path = OptionPath::new(vec!["services".into(), "nginx".into(), "enable".into()]);

        insert_at_path(&mut attrs, &path, Value::Bool(true));

        assert!(attrs.contains_key("services"));
        if let Some(Value::Attrs(services)) = attrs.get("services") {
            assert!(services.contains_key("nginx"));
            if let Some(Value::Attrs(nginx)) = services.get("nginx") {
                assert_eq!(nginx.get("enable"), Some(&Value::Bool(true)));
            }
        }
    }

    #[test]
    fn test_has_value_at_path() {
        let mut attrs = IndexMap::new();
        let path = OptionPath::new(vec!["a".into(), "b".into()]);

        insert_at_path(&mut attrs, &path, Value::Int(42));

        assert!(has_value_at_path(&attrs, &path));
        assert!(!has_value_at_path(&attrs, &OptionPath::new(vec!["a".into(), "c".into()])));
    }
}
