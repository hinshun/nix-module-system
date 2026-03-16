//! Submodule type for nested module evaluation.

use super::{Definition, MergeResult, NixType, OptionDoc, OptionPath, Value};
use crate::errors::TypeError;
use indexmap::IndexMap;
use std::collections::HashMap;

/// Arguments passed to module functions.
///
/// In Nix, modules can be functions that receive these arguments:
/// ```nix
/// { config, options, lib, pkgs, name, ... }: { ... }
/// ```
#[derive(Debug, Clone)]
pub struct ModuleArgs {
    /// The current configuration (merged so far)
    pub config: Value,
    /// The option declarations
    pub options: Value,
    /// The lib functions (mkOption, types, etc.)
    pub lib: Value,
    /// The package set (optional, not always available)
    pub pkgs: Option<Value>,
    /// The name of the submodule instance (for attrsOf submodule)
    pub name: String,
}

impl Default for ModuleArgs {
    fn default() -> Self {
        Self {
            config: Value::Attrs(IndexMap::new()),
            options: Value::Attrs(IndexMap::new()),
            lib: Value::Attrs(IndexMap::new()),
            pkgs: None,
            name: String::new(),
        }
    }
}

impl ModuleArgs {
    /// Create new module args with a name
    pub fn with_name(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Convert to a Value::Attrs for representing in unevaluated modules
    pub fn to_value(&self) -> Value {
        let mut attrs = IndexMap::new();
        attrs.insert("config".to_string(), self.config.clone());
        attrs.insert("options".to_string(), self.options.clone());
        attrs.insert("lib".to_string(), self.lib.clone());
        if let Some(pkgs) = &self.pkgs {
            attrs.insert("pkgs".to_string(), pkgs.clone());
        }
        attrs.insert("name".to_string(), Value::String(self.name.clone()));
        Value::Attrs(attrs)
    }
}

/// Represents a module that needs Nix-side evaluation.
///
/// Since we cannot actually call Nix lambdas from Rust, we represent
/// unevaluated modules as special attribute sets that can be processed
/// by the Nix evaluator.
#[derive(Debug, Clone)]
pub struct UnevaluatedModule {
    /// The original lambda (represented as marker)
    pub _type: String,
    /// The arguments that would be passed
    pub args: ModuleArgs,
    /// The file where this module was defined
    pub file: Option<std::path::PathBuf>,
}

impl UnevaluatedModule {
    /// Create a new unevaluated module marker
    pub fn new(args: ModuleArgs) -> Self {
        Self {
            _type: "unevaluated-module".to_string(),
            args,
            file: None,
        }
    }

    /// Convert to a Value::Attrs representation
    pub fn to_value(&self) -> Value {
        let mut attrs = IndexMap::new();
        attrs.insert("_type".to_string(), Value::String(self._type.clone()));
        attrs.insert("args".to_string(), self.args.to_value());
        if let Some(file) = &self.file {
            attrs.insert("file".to_string(), Value::Path(file.clone()));
        }
        Value::Attrs(attrs)
    }
}

/// A module that can be embedded as a type in another module.
#[derive(Debug, Clone)]
pub struct Module {
    /// The module's options
    pub options: IndexMap<String, OptionDecl>,
    /// Default config values
    pub defaults: IndexMap<String, Value>,
    /// Module file for error reporting
    pub file: Option<std::path::PathBuf>,
}

impl Module {
    /// Create a new empty module
    pub fn new() -> Self {
        Self {
            options: IndexMap::new(),
            defaults: IndexMap::new(),
            file: None,
        }
    }

    /// Add an option to the module
    pub fn with_option(mut self, name: &str, decl: OptionDecl) -> Self {
        self.options.insert(name.to_string(), decl);
        self
    }

    /// Add a default config value
    pub fn with_default(mut self, name: &str, value: Value) -> Self {
        self.defaults.insert(name.to_string(), value);
        self
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

/// An option declaration within a module
#[derive(Debug, Clone)]
pub struct OptionDecl {
    /// The option's type
    pub type_: Box<dyn NixType>,
    /// Default value
    pub default: Option<Value>,
    /// Example value
    pub example: Option<Value>,
    /// Description
    pub description: Option<String>,
    /// Whether this option is internal
    pub internal: bool,
    /// Whether this option is visible
    pub visible: bool,
    /// Whether this option is read-only
    pub read_only: bool,
}

impl OptionDecl {
    /// Create a new option declaration
    pub fn new(type_: Box<dyn NixType>) -> Self {
        Self {
            type_,
            default: None,
            example: None,
            description: None,
            internal: false,
            visible: true,
            read_only: false,
        }
    }

    /// Set the default value
    pub fn with_default(mut self, value: Value) -> Self {
        self.default = Some(value);
        self
    }

    /// Set the description
    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    /// Try to merge two option declarations.
    ///
    /// Returns a merged declaration if the types are compatible.
    pub fn try_merge(&self, other: &OptionDecl) -> Option<OptionDecl> {
        // Check if types have the same name (basic compatibility)
        if self.type_.name() != other.type_.name() {
            return None;
        }

        // Use the first declaration's type, but merge metadata
        Some(OptionDecl {
            type_: self.type_.clone_box(),
            // Prefer explicit default over None
            default: self.default.clone().or_else(|| other.default.clone()),
            example: self.example.clone().or_else(|| other.example.clone()),
            description: self
                .description
                .clone()
                .or_else(|| other.description.clone()),
            // Use more restrictive settings
            internal: self.internal || other.internal,
            visible: self.visible && other.visible,
            read_only: self.read_only || other.read_only,
        })
    }
}

/// Submodule type that evaluates embedded modules
#[derive(Debug)]
pub struct Submodule {
    /// Static modules that define the submodule's options
    modules: Vec<Module>,
    /// Whether shorthand syntax only defines config (not options)
    shorthand_only_defines_config: bool,
    /// Freeform type for undefined options
    freeform_type: Option<Box<dyn NixType>>,
    /// Modules that need Nix-side evaluation (lambdas)
    unevaluated_modules: Vec<UnevaluatedModule>,
}

impl Submodule {
    /// Create a new submodule type
    pub fn new(modules: Vec<Module>) -> Self {
        Self {
            modules,
            shorthand_only_defines_config: true,
            freeform_type: None,
            unevaluated_modules: Vec::new(),
        }
    }

    /// Create a submodule that interprets values as options
    pub fn with_options_shorthand(mut self) -> Self {
        self.shorthand_only_defines_config = false;
        self
    }

    /// Set a freeform type for undefined options
    ///
    /// When freeformType is set, options not defined in the module
    /// are still allowed and will be type-checked against this type.
    pub fn with_freeform_type(mut self, ty: Box<dyn NixType>) -> Self {
        self.freeform_type = Some(ty);
        self
    }

    /// Get the freeform type if set
    pub fn freeform_type(&self) -> Option<&dyn NixType> {
        self.freeform_type.as_ref().map(|t| t.as_ref())
    }

    /// Get the merged options from all modules
    fn merged_options(&self) -> IndexMap<String, OptionDecl> {
        let mut result: IndexMap<String, OptionDecl> = IndexMap::new();

        for module in &self.modules {
            for (name, decl) in &module.options {
                if let Some(existing) = result.get(name) {
                    // Try to merge option declarations
                    if let Some(merged) = existing.try_merge(decl) {
                        result.insert(name.clone(), merged);
                    }
                    // If merge fails, keep the first declaration
                    // (proper error handling would report this)
                } else {
                    result.insert(name.clone(), decl.clone());
                }
            }
        }

        result
    }

    /// Normalize a module value, handling shorthand syntax.
    ///
    /// If a module is pure attrs without options/config keys and
    /// shorthand_only_defines_config is true, wrap it in { config = <attrs>; }
    fn normalize_module_value(&self, value: Value) -> Value {
        match value {
            Value::Attrs(attrs) => {
                // Check if this is already a proper module structure
                let has_options = attrs.contains_key("options");
                let has_config = attrs.contains_key("config");
                let has_imports = attrs.contains_key("imports");

                if has_options || has_config || has_imports {
                    // Already a proper module structure
                    Value::Attrs(attrs)
                } else if self.shorthand_only_defines_config {
                    // Shorthand syntax: treat as pure config
                    let mut wrapper = IndexMap::new();
                    wrapper.insert("config".to_string(), Value::Attrs(attrs));
                    Value::Attrs(wrapper)
                } else {
                    // Not shorthand mode, treat as options
                    let mut wrapper = IndexMap::new();
                    wrapper.insert("options".to_string(), Value::Attrs(attrs));
                    Value::Attrs(wrapper)
                }
            }
            other => other,
        }
    }

    /// Apply a module function, returning an unevaluated module marker.
    ///
    /// Since we can't actually call Nix lambdas from Rust, this creates
    /// a representation that can be evaluated on the Nix side.
    #[cfg(test)]
    fn apply_module_function(&mut self, args: ModuleArgs, file: Option<std::path::PathBuf>) -> Value {
        let mut unevaluated = UnevaluatedModule::new(args);
        unevaluated.file = file;

        // Track for later evaluation
        self.unevaluated_modules.push(unevaluated.clone());

        unevaluated.to_value()
    }

    /// Check if a value represents an unevaluated module
    pub fn is_unevaluated_module(value: &Value) -> bool {
        if let Value::Attrs(attrs) = value {
            if let Some(Value::String(t)) = attrs.get("_type") {
                return t == "unevaluated-module";
            }
        }
        false
    }

    /// Get modules that need Nix-side evaluation
    pub fn unevaluated_modules(&self) -> &[UnevaluatedModule] {
        &self.unevaluated_modules
    }

    /// Check if there are modules pending Nix evaluation
    pub fn has_unevaluated_modules(&self) -> bool {
        !self.unevaluated_modules.is_empty()
    }

    /// Extract config from a normalized module value
    fn extract_config<'a>(&self, normalized: &'a Value) -> Option<&'a IndexMap<String, Value>> {
        match normalized {
            Value::Attrs(attrs) => {
                if let Some(Value::Attrs(config)) = attrs.get("config") {
                    Some(config)
                } else {
                    // If no config key, the whole attrs is the config (shorthand already applied)
                    Some(attrs)
                }
            }
            _ => None,
        }
    }
}

impl Clone for Submodule {
    fn clone(&self) -> Self {
        Self {
            modules: self.modules.clone(),
            shorthand_only_defines_config: self.shorthand_only_defines_config,
            freeform_type: self.freeform_type.as_ref().map(|t| t.clone_box()),
            unevaluated_modules: self.unevaluated_modules.clone(),
        }
    }
}

impl NixType for Submodule {
    fn name(&self) -> &str {
        "submodule"
    }

    fn description(&self) -> String {
        "submodule".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Attrs(_) => Ok(()), // Shallow check only
            Value::Lambda => Ok(()),   // Functions are valid module values
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: format!("{:?}", value),
                value: Some(value.clone()),
            }),
        }
    }

    fn merge(&self, loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        if defs.is_empty() {
            // Return empty attrs if no definitions
            return Ok(MergeResult::from_default(Value::Attrs(IndexMap::new())));
        }

        // Get the merged option declarations
        let options = self.merged_options();

        // Collect definitions for each option
        let mut by_option: IndexMap<String, Vec<Definition>> = IndexMap::new();

        // Track unevaluated modules
        let mut has_lambdas = false;
        let mut unevaluated_markers: Vec<Value> = Vec::new();

        for def in &defs {
            match &def.value {
                Value::Attrs(attrs) => {
                    // Check if this is an unevaluated module marker
                    if Self::is_unevaluated_module(&def.value) {
                        unevaluated_markers.push(def.value.clone());
                        continue;
                    }

                    // Normalize the module (handle shorthand syntax)
                    let normalized = self.normalize_module_value(def.value.clone());

                    // Extract config from normalized module
                    if let Some(config) = self.extract_config(&normalized) {
                        for (name, value) in config {
                            by_option.entry(name.clone()).or_default().push(Definition {
                                file: def.file.clone(),
                                span: def.span.clone(),
                                value: value.clone(),
                                priority: def.priority,
                            });
                        }
                    } else {
                        // Direct attrs without config wrapper
                        for (name, value) in attrs {
                            by_option.entry(name.clone()).or_default().push(Definition {
                                file: def.file.clone(),
                                span: def.span.clone(),
                                value: value.clone(),
                                priority: def.priority,
                            });
                        }
                    }
                }
                Value::Lambda => {
                    // Mark that we have lambdas that need Nix-side evaluation
                    has_lambdas = true;

                    // Create an unevaluated module marker
                    let args = ModuleArgs::default();
                    let unevaluated = UnevaluatedModule {
                        _type: "unevaluated-module".to_string(),
                        args,
                        file: Some(def.file.clone()),
                    };
                    unevaluated_markers.push(unevaluated.to_value());
                }
                _ => {
                    return Err(TypeError::Mismatch {
                        expected: self.description(),
                        found: format!("{:?}", def.value),
                        value: Some(def.value.clone()),
                    });
                }
            }
        }

        // Evaluate each option
        let mut result = IndexMap::new();

        for (name, decl) in &options {
            let opt_loc = loc.child(name);

            if let Some(opt_defs) = by_option.shift_remove(name) {
                let merged = decl.type_.merge(&opt_loc, opt_defs)?;
                result.insert(name.clone(), merged.value);
            } else if let Some(default) = &decl.default {
                result.insert(name.clone(), default.clone());
            }
            // If no definition and no default, the option is undefined
        }

        // Handle undefined options
        for (name, defs) in by_option {
            if let Some(ref freeform) = self.freeform_type {
                // Use freeform type to merge undefined options
                let opt_loc = loc.child(&name);
                let merged = freeform.merge(&opt_loc, defs)?;
                result.insert(name, merged.value);
            } else {
                return Err(TypeError::UndefinedOption {
                    path: loc.child(&name),
                    available: options.keys().cloned().collect(),
                });
            }
        }

        // If we have lambdas, include unevaluated module markers
        if has_lambdas || !unevaluated_markers.is_empty() {
            result.insert(
                "_unevaluatedModules".to_string(),
                Value::List(unevaluated_markers),
            );
        }

        Ok(MergeResult::new(Value::Attrs(result)))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn empty_value(&self) -> Option<Value> {
        Some(Value::Attrs(IndexMap::new()))
    }

    fn get_sub_options(&self, prefix: &OptionPath) -> HashMap<OptionPath, OptionDoc> {
        let mut result = HashMap::new();

        for (name, decl) in self.merged_options() {
            let path = prefix.child(&name);

            result.insert(
                path.clone(),
                OptionDoc {
                    path: path.clone(),
                    type_desc: decl.type_.description(),
                    default: decl.default.clone(),
                    example: decl.example.clone(),
                    description: decl.description.clone(),
                    internal: decl.internal,
                    visible: decl.visible,
                    read_only: decl.read_only,
                },
            );

            // Recurse into nested submodules
            result.extend(decl.type_.get_sub_options(&path));
        }

        result
    }

    fn nested_types(&self) -> HashMap<String, Box<dyn NixType>> {
        let mut map = HashMap::new();
        for (name, decl) in self.merged_options() {
            map.insert(name, decl.type_.clone_box());
        }
        map
    }
}

/// Deferred module type for lazy module import
#[derive(Debug, Clone)]
pub struct DeferredModule {
    static_modules: Vec<Module>,
}

impl DeferredModule {
    /// Create a new deferred module type
    pub fn new() -> Self {
        Self {
            static_modules: Vec::new(),
        }
    }

    /// Create with static modules for documentation
    pub fn with_modules(modules: Vec<Module>) -> Self {
        Self {
            static_modules: modules,
        }
    }
}

impl Default for DeferredModule {
    fn default() -> Self {
        Self::new()
    }
}

impl NixType for DeferredModule {
    fn name(&self) -> &str {
        "deferredModule"
    }

    fn description(&self) -> String {
        "module".to_string()
    }

    fn check(&self, value: &Value) -> Result<(), TypeError> {
        match value {
            Value::Attrs(_) | Value::Lambda => Ok(()),
            Value::Path(_) => Ok(()), // Paths to module files
            _ => Err(TypeError::Mismatch {
                expected: self.description(),
                found: format!("{:?}", value),
                value: Some(value.clone()),
            }),
        }
    }

    fn merge(&self, _loc: &OptionPath, defs: Vec<Definition>) -> Result<MergeResult, TypeError> {
        // Deferred modules collect imports, they don't merge values
        let imports: Vec<Value> = defs.into_iter().map(|d| d.value).collect();

        Ok(MergeResult::new(Value::Attrs({
            let mut attrs = IndexMap::new();
            attrs.insert("imports".to_string(), Value::List(imports));
            attrs
        })))
    }

    fn clone_box(&self) -> Box<dyn NixType> {
        Box::new(self.clone())
    }

    fn get_sub_options(&self, prefix: &OptionPath) -> HashMap<OptionPath, OptionDoc> {
        // Return options from static modules for documentation
        let submodule = Submodule::new(self.static_modules.clone());
        submodule.get_sub_options(prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AttrsOf, Bool, Int, Str};

    #[test]
    fn test_submodule_basic() {
        let module = Module::new()
            .with_option(
                "enable",
                OptionDecl::new(Box::new(Bool)).with_default(Value::Bool(false)),
            )
            .with_option(
                "name",
                OptionDecl::new(Box::new(Str)).with_description("The name"),
            );

        let ty = Submodule::new(vec![module]);

        // Check valid attrs
        let mut attrs = IndexMap::new();
        attrs.insert("enable".to_string(), Value::Bool(true));
        attrs.insert("name".to_string(), Value::String("test".into()));
        assert!(ty.check(&Value::Attrs(attrs)).is_ok());
    }

    #[test]
    fn test_submodule_merge() {
        let module = Module::new()
            .with_option(
                "enable",
                OptionDecl::new(Box::new(Bool)).with_default(Value::Bool(false)),
            )
            .with_option("name", OptionDecl::new(Box::new(Str)));

        let ty = Submodule::new(vec![module]);

        let mut attrs1 = IndexMap::new();
        attrs1.insert("enable".to_string(), Value::Bool(true));

        let mut attrs2 = IndexMap::new();
        attrs2.insert("name".to_string(), Value::String("test".into()));

        let defs = vec![
            Definition::new(Value::Attrs(attrs1)),
            Definition::new(Value::Attrs(attrs2)),
        ];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.get("enable"), Some(&Value::Bool(true)));
            assert_eq!(attrs.get("name"), Some(&Value::String("test".into())));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_submodule_undefined_option() {
        let module = Module::new().with_option("enable", OptionDecl::new(Box::new(Bool)));

        let ty = Submodule::new(vec![module]);

        let mut attrs = IndexMap::new();
        attrs.insert("invalid".to_string(), Value::Bool(true));

        let defs = vec![Definition::new(Value::Attrs(attrs))];

        let result = ty.merge(&OptionPath::root(), defs);
        assert!(matches!(result, Err(TypeError::UndefinedOption { .. })));
    }

    #[test]
    fn test_shorthand_module_syntax() {
        // Create a submodule with shorthand enabled (default)
        let module = Module::new()
            .with_option("port", OptionDecl::new(Box::new(Int)))
            .with_option("host", OptionDecl::new(Box::new(Str)));

        let ty = Submodule::new(vec![module]);

        // Shorthand syntax: { port = 8080; host = "localhost"; }
        // Should be treated as config, not options
        let mut shorthand_config = IndexMap::new();
        shorthand_config.insert("port".to_string(), Value::Int(8080));
        shorthand_config.insert("host".to_string(), Value::String("localhost".into()));

        let defs = vec![Definition::new(Value::Attrs(shorthand_config))];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.get("port"), Some(&Value::Int(8080)));
            assert_eq!(attrs.get("host"), Some(&Value::String("localhost".into())));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_shorthand_with_explicit_config() {
        // Even with shorthand enabled, explicit config key should work
        let module = Module::new()
            .with_option("port", OptionDecl::new(Box::new(Int)));

        let ty = Submodule::new(vec![module]);

        // Explicit config: { config = { port = 8080; }; }
        let mut config_inner = IndexMap::new();
        config_inner.insert("port".to_string(), Value::Int(8080));

        let mut explicit_config = IndexMap::new();
        explicit_config.insert("config".to_string(), Value::Attrs(config_inner));

        let defs = vec![Definition::new(Value::Attrs(explicit_config))];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.get("port"), Some(&Value::Int(8080)));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_freeform_type_allows_extra_attrs() {
        // Create a submodule with freeformType = attrsOf str
        let module = Module::new()
            .with_option("enable", OptionDecl::new(Box::new(Bool)));

        let ty = Submodule::new(vec![module])
            .with_freeform_type(Box::new(Str));

        // Config with both declared and extra options
        let mut config = IndexMap::new();
        config.insert("enable".to_string(), Value::Bool(true));
        config.insert("extraOption".to_string(), Value::String("allowed".into()));
        config.insert("anotherExtra".to_string(), Value::String("also allowed".into()));

        let defs = vec![Definition::new(Value::Attrs(config))];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.get("enable"), Some(&Value::Bool(true)));
            assert_eq!(attrs.get("extraOption"), Some(&Value::String("allowed".into())));
            assert_eq!(attrs.get("anotherExtra"), Some(&Value::String("also allowed".into())));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_freeform_type_with_attrsof() {
        // freeformType = attrsOf (attrsOf str)
        let module = Module::new()
            .with_option("name", OptionDecl::new(Box::new(Str)));

        let freeform = AttrsOf::new(Box::new(Str));
        let ty = Submodule::new(vec![module])
            .with_freeform_type(Box::new(freeform));

        let mut extra_attrs = IndexMap::new();
        extra_attrs.insert("key1".to_string(), Value::String("value1".into()));
        extra_attrs.insert("key2".to_string(), Value::String("value2".into()));

        let mut config = IndexMap::new();
        config.insert("name".to_string(), Value::String("myname".into()));
        config.insert("extraSettings".to_string(), Value::Attrs(extra_attrs));

        let defs = vec![Definition::new(Value::Attrs(config))];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.get("name"), Some(&Value::String("myname".into())));
            if let Some(Value::Attrs(extra)) = attrs.get("extraSettings") {
                assert_eq!(extra.get("key1"), Some(&Value::String("value1".into())));
            } else {
                panic!("Expected extraSettings to be Attrs");
            }
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_multiple_modules_compatible_options() {
        // Two modules declaring the same option with compatible types
        let module1 = Module::new()
            .with_option(
                "enable",
                OptionDecl::new(Box::new(Bool)).with_description("Enable the service"),
            );

        let module2 = Module::new()
            .with_option(
                "enable",
                OptionDecl::new(Box::new(Bool)).with_default(Value::Bool(false)),
            );

        let ty = Submodule::new(vec![module1, module2]);

        // The merged options should have both description and default
        let merged_options = ty.merged_options();
        let enable_opt = merged_options.get("enable").unwrap();

        assert!(enable_opt.description.is_some());
        assert_eq!(enable_opt.description.as_ref().unwrap(), "Enable the service");
        assert!(enable_opt.default.is_some());
        assert_eq!(enable_opt.default, Some(Value::Bool(false)));
    }

    #[test]
    fn test_multiple_modules_different_options() {
        // Two modules declaring different options
        let module1 = Module::new()
            .with_option("port", OptionDecl::new(Box::new(Int)));

        let module2 = Module::new()
            .with_option("host", OptionDecl::new(Box::new(Str)));

        let ty = Submodule::new(vec![module1, module2]);

        let merged_options = ty.merged_options();
        assert!(merged_options.contains_key("port"));
        assert!(merged_options.contains_key("host"));
    }

    #[test]
    fn test_lambda_creates_unevaluated_marker() {
        let module = Module::new()
            .with_option("enable", OptionDecl::new(Box::new(Bool)));

        let ty = Submodule::new(vec![module]);

        // Mix of normal config and lambda
        let mut attrs = IndexMap::new();
        attrs.insert("enable".to_string(), Value::Bool(true));

        let defs = vec![
            Definition::new(Value::Attrs(attrs)),
            Definition::new(Value::Lambda),
        ];

        let result = ty.merge(&OptionPath::root(), defs).unwrap();

        if let Value::Attrs(attrs) = result.value {
            assert_eq!(attrs.get("enable"), Some(&Value::Bool(true)));
            // Should have unevaluated modules marker
            assert!(attrs.contains_key("_unevaluatedModules"));
            if let Some(Value::List(markers)) = attrs.get("_unevaluatedModules") {
                assert_eq!(markers.len(), 1);
                // Check the marker structure
                if let Value::Attrs(marker) = &markers[0] {
                    assert_eq!(marker.get("_type"), Some(&Value::String("unevaluated-module".into())));
                } else {
                    panic!("Expected marker to be Attrs");
                }
            } else {
                panic!("Expected _unevaluatedModules to be List");
            }
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_is_unevaluated_module() {
        let mut marker = IndexMap::new();
        marker.insert("_type".to_string(), Value::String("unevaluated-module".into()));

        assert!(Submodule::is_unevaluated_module(&Value::Attrs(marker)));

        let mut not_marker = IndexMap::new();
        not_marker.insert("_type".to_string(), Value::String("other".into()));

        assert!(!Submodule::is_unevaluated_module(&Value::Attrs(not_marker)));
        assert!(!Submodule::is_unevaluated_module(&Value::Bool(true)));
    }

    #[test]
    fn test_module_args() {
        let args = ModuleArgs::with_name("myservice");
        assert_eq!(args.name, "myservice");

        let value = args.to_value();
        if let Value::Attrs(attrs) = value {
            assert_eq!(attrs.get("name"), Some(&Value::String("myservice".into())));
            assert!(attrs.contains_key("config"));
            assert!(attrs.contains_key("options"));
            assert!(attrs.contains_key("lib"));
        } else {
            panic!("Expected Attrs");
        }
    }

    #[test]
    fn test_option_decl_merge() {
        let opt1 = OptionDecl::new(Box::new(Bool))
            .with_description("First description");

        let opt2 = OptionDecl::new(Box::new(Bool))
            .with_default(Value::Bool(false));

        let merged = opt1.try_merge(&opt2).unwrap();

        // Should have description from first and default from second
        assert_eq!(merged.description, Some("First description".to_string()));
        assert_eq!(merged.default, Some(Value::Bool(false)));
    }

    #[test]
    fn test_option_decl_merge_incompatible_types() {
        let opt1 = OptionDecl::new(Box::new(Bool));
        let opt2 = OptionDecl::new(Box::new(Str));

        // Different types should not merge
        assert!(opt1.try_merge(&opt2).is_none());
    }

    #[test]
    fn test_apply_module_function() {
        let module = Module::new();
        let mut ty = Submodule::new(vec![module]);

        let args = ModuleArgs::with_name("test");
        let result = ty.apply_module_function(args, None);

        // Should create an unevaluated module marker
        assert!(Submodule::is_unevaluated_module(&result));

        // Should track the unevaluated module
        assert!(ty.has_unevaluated_modules());
        assert_eq!(ty.unevaluated_modules().len(), 1);
    }
}
