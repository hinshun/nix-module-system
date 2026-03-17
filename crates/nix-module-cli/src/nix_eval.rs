//! Nix evaluator using nix-bindings-rust.
//!
//! Embeds a Nix EvalState in the CLI process. Loads the nix-module-plugin
//! shared library to register primops as builtins, then evaluates
//! `evalModules` from `nix/lib.nix`.

use nix_module_system::types::Value;

use nix_bindings_expr::eval_state::{gc_register_my_thread, init, EvalState, ThreadRegistrationGuard};
use nix_bindings_store::store::Store;

use std::path::PathBuf;

/// Configuration for the Nix evaluator.
#[derive(Debug, Clone)]
pub struct NixEvalConfig {
    /// Path to the nix/lib.nix file.
    pub lib_path: PathBuf,
    /// Path to the plugin shared library.
    pub plugin_path: PathBuf,
}

impl NixEvalConfig {
    /// Create a new config, discovering paths automatically.
    pub fn discover(
        lib_path: Option<PathBuf>,
        plugin_path: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let lib_path = lib_path
            .or_else(|| std::env::var("NMS_LIB_PATH").ok().map(PathBuf::from))
            .or_else(find_lib_nix)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot find nix/lib.nix. Set --lib-path, NMS_LIB_PATH, or run from the project root."
                )
            })?;

        if !lib_path.exists() {
            anyhow::bail!("lib.nix not found at: {}", lib_path.display());
        }

        let plugin_path = plugin_path
            .or_else(|| std::env::var("NMS_PLUGIN_PATH").ok().map(PathBuf::from))
            .or_else(find_plugin)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Cannot find nix-module-plugin shared library. \
                     Set --plugin-path, NMS_PLUGIN_PATH, or build with `cargo build -p nix-module-plugin`."
                )
            })?;

        if !plugin_path.exists() {
            anyhow::bail!("Plugin not found at: {}", plugin_path.display());
        }

        Ok(Self {
            lib_path,
            plugin_path,
        })
    }
}

/// Result of evaluating modules.
#[derive(Debug)]
pub struct NixEvalResult {
    /// The merged configuration.
    pub config: Value,
}

/// High-level Nix evaluator backed by nix-bindings-rust.
///
/// On creation, loads the plugin .so to register primops as builtins,
/// then creates an EvalState with those builtins available.
pub struct NixEvaluator {
    eval_state: EvalState,
    _guard: ThreadRegistrationGuard,
    /// Keep the library handle alive so symbols remain loaded.
    _plugin: libloading::Library,
}

impl NixEvaluator {
    /// Create a new evaluator, loading the plugin and initializing Nix.
    ///
    /// Order matters:
    /// 1. Initialize Nix library
    /// 2. Load plugin .so (calls nix_plugin_entry, registers builtins)
    /// 3. Create EvalState (builtins now include __nms_* primops)
    pub fn new(eval_config: &NixEvalConfig) -> anyhow::Result<Self> {
        // Step 1: Initialize Nix (idempotent)
        init().map_err(|e| anyhow::anyhow!("Failed to initialize Nix: {}", e))?;

        // Step 2: Load plugin .so and call nix_plugin_entry()
        let plugin = unsafe {
            let lib = libloading::Library::new(&eval_config.plugin_path).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to load plugin {}: {}",
                    eval_config.plugin_path.display(),
                    e
                )
            })?;

            // Call the plugin entry point to register primops
            let entry: libloading::Symbol<unsafe extern "C" fn()> =
                lib.get(b"nix_plugin_entry").map_err(|e| {
                    anyhow::anyhow!(
                        "Plugin {} missing nix_plugin_entry symbol: {}",
                        eval_config.plugin_path.display(),
                        e
                    )
                })?;
            entry();

            lib
        };

        // Step 3: Create EvalState (primops are now registered as builtins)
        let guard = gc_register_my_thread()
            .map_err(|e| anyhow::anyhow!("Failed to register GC thread: {}", e))?;

        let store = Store::open(None, Vec::<(&str, &str)>::new())
            .map_err(|e| anyhow::anyhow!("Failed to open Nix store: {}", e))?;

        let eval_state = EvalState::new(store, Vec::<&str>::new())
            .map_err(|e| anyhow::anyhow!("Failed to create EvalState: {}", e))?;

        Ok(Self {
            eval_state,
            _guard: guard,
            _plugin: plugin,
        })
    }

    /// Evaluate a Nix expression string to a string result.
    pub fn eval_string(&mut self, expr: &str) -> anyhow::Result<String> {
        let nix_value = self
            .eval_state
            .eval_from_string(expr, "<cli>")
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        self.eval_state
            .require_string(&nix_value)
            .map_err(|e| anyhow::anyhow!("{}", e))
    }

    /// Evaluate a Nix expression and return its JSON serialization as a Value.
    ///
    /// Wraps the expression in `builtins.toJSON (...)` so Nix handles
    /// serialization (including lazy forcing), then parses the resulting
    /// JSON string on the Rust side.
    pub fn eval_json(&mut self, expr: &str) -> anyhow::Result<Value> {
        let json_str = self.eval_string(&format!("builtins.toJSON ({})", expr))?;

        let json: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse Nix JSON output: {}", e))?;

        Ok(nix_module_system::nix::json_to_value(&json))
    }
}

/// Evaluate modules using the embedded Nix evaluator.
pub fn eval_modules(
    eval_config: &NixEvalConfig,
    module_files: &[PathBuf],
) -> anyhow::Result<NixEvalResult> {
    let mut evaluator = NixEvaluator::new(eval_config)?;

    let lib_path = eval_config.lib_path.display();
    let modules = module_files
        .iter()
        .map(|p| format!("    {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");

    let expr = format!(
        r#"let
  lib = import {lib_path};
  result = lib.evalModules {{
    modules = [
{modules}
    ];
  }};
in result.config"#
    );

    tracing::debug!("Evaluating:\n{}", expr);

    let config = evaluator.eval_json(&expr)?;
    Ok(NixEvalResult { config })
}

/// Quick test: evaluate `builtins.nms_version` to verify primops work.
pub fn test_primops(eval_config: &NixEvalConfig) -> anyhow::Result<String> {
    let mut evaluator = NixEvaluator::new(eval_config)?;
    evaluator.eval_string("builtins.nms_version")
}

/// Check modules for errors by forcing full evaluation.
pub fn check_modules(
    eval_config: &NixEvalConfig,
    module_files: &[PathBuf],
) -> anyhow::Result<()> {
    eval_modules(eval_config, module_files)?;
    Ok(())
}

/// Evaluate modules and extract option declarations.
pub fn eval_options(
    eval_config: &NixEvalConfig,
    module_files: &[PathBuf],
) -> anyhow::Result<Value> {
    let mut evaluator = NixEvaluator::new(eval_config)?;

    let lib_path = eval_config.lib_path.display();
    let modules = module_files
        .iter()
        .map(|p| format!("    {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");

    let expr = format!(
        r#"let
  lib = import {lib_path};
  result = lib.evalModules {{
    modules = [
{modules}
    ];
  }};
  formatOption = name: opt: {{
    path = name;
    type = opt.type.name or "unknown";
    default = opt.default or null;
    description = opt.description or null;
    internal = opt.internal or false;
  }};
in lib.mapAttrs formatOption result.options"#
    );

    tracing::debug!("Evaluating:\n{}", expr);

    evaluator.eval_json(&expr)
}

// ---------------------------------------------------------------------------
// Path discovery
// ---------------------------------------------------------------------------

/// Try to find nix/lib.nix relative to the executable or CWD.
fn find_lib_nix() -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        let candidate = cwd.join("nix/lib.nix");
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let candidate = exe_dir.join("../share/nix-module-system/nix/lib.nix");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Try to find the plugin shared library.
fn find_plugin() -> Option<PathBuf> {
    // Try build output locations (for development)
    let candidates = [
        "target/release/libnix_module_plugin.so",
        "target/debug/libnix_module_plugin.so",
        "target/release/libnix_module_plugin.dylib",
        "target/debug/libnix_module_plugin.dylib",
    ];

    if let Ok(cwd) = std::env::current_dir() {
        for candidate in &candidates {
            let path = cwd.join(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    // Try installed locations relative to the binary
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for ext in &["so", "dylib"] {
                let candidate = exe_dir.join(format!("../lib/libnix_module_plugin.{}", ext));
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_lib_nix_from_cwd() {
        if let Some(path) = find_lib_nix() {
            assert!(path.exists());
            assert!(path.to_string_lossy().contains("lib.nix"));
        }
    }
}
