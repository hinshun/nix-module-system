//! Nix builtins plugin for the high-performance module system.
//!
//! This crate is a cdylib loaded by the Nix evaluator via `--plugin-files`.
//! It registers primop builtins that the Nix-side `evalModules` orchestrator
//! calls for compute-intensive operations (merging, type checking, conditionals).
//!
//! ## Registered primops
//!
//! | Builtin | Arity | Purpose |
//! |---------|-------|---------|
//! | `__nms_version` | 0 | Returns plugin version string |
//! | `__nms_checkType` | 2 | Type-check a value against a type descriptor |
//! | `__nms_processConditionals` | 1 | Flatten mkIf/mkMerge/mkOverride wrappers |
//! | `__nms_mergeDefinitions` | 3 | Merge multiple definitions for an option |
//!
//! ## Usage
//!
//! ```bash
//! nix eval --plugin-files ./libnix_module_plugin.so --expr 'builtins.__nms_version'
//! ```

mod ffi;
mod primops;
mod type_resolve;

use std::ffi::c_void;
use std::ptr;

/// Nix plugin entry point.
///
/// Called by the Nix evaluator when loading this shared library via `--plugin-files`.
/// Registers all module system primops as builtins.
///
/// # Safety
///
/// Called by the Nix runtime. Must only register primops and return.
#[no_mangle]
pub unsafe extern "C" fn nix_plugin_entry() {
    let ctx = ffi::nix_c_context_create();
    if ctx.is_null() {
        eprintln!("nix-module-plugin: failed to create nix context");
        return;
    }

    // Register __nms_version (arity 0)
    register_primop(
        ctx,
        "__nms_version",
        0,
        &[],
        "Returns the nix-module-system plugin version.",
        primops::primop_version,
    );

    // Register __nms_checkType (arity 2)
    register_primop(
        ctx,
        "__nms_checkType",
        2,
        &["typeDesc", "value"],
        "Type-check a value against a type descriptor. Returns true/false.",
        primops::primop_check_type,
    );

    // Register __nms_processConditionals (arity 1)
    register_primop(
        ctx,
        "__nms_processConditionals",
        1,
        &["value"],
        "Flatten mkIf/mkMerge/mkOverride wrappers into active definitions.",
        primops::primop_process_conditionals,
    );

    // Register __nms_mergeDefinitions (arity 3)
    register_primop(
        ctx,
        "__nms_mergeDefinitions",
        3,
        &["name", "typeDesc", "defs"],
        "Merge multiple definitions for an option using its type's merge strategy.",
        primops::primop_merge_definitions,
    );

    ffi::nix_c_context_free(ctx);
}

/// Helper to register a single primop.
///
/// # Safety
///
/// `ctx` must be a valid Nix context.
unsafe fn register_primop(
    ctx: *mut ffi::nix_c_context,
    name: &str,
    arity: i32,
    arg_names: &[&str],
    doc: &str,
    fun: ffi::PrimOpFun,
) {
    use std::ffi::CString;

    let name_c = CString::new(name).expect("primop name");
    let doc_c = CString::new(doc).expect("primop doc");

    // Build args array (null-terminated array of C strings)
    let arg_cstrings: Vec<CString> = arg_names
        .iter()
        .map(|a| CString::new(*a).expect("arg name"))
        .collect();
    let mut arg_ptrs: Vec<*const std::ffi::c_char> =
        arg_cstrings.iter().map(|cs| cs.as_ptr()).collect();
    arg_ptrs.push(ptr::null()); // null terminator

    let primop = ffi::nix_alloc_primop(
        ctx,
        fun,
        arity,
        name_c.as_ptr(),
        arg_ptrs.as_mut_ptr() as *mut *const std::ffi::c_char,
        doc_c.as_ptr(),
        ptr::null_mut::<c_void>(),
    );

    if primop.is_null() {
        eprintln!("nix-module-plugin: failed to allocate primop '{}'", name);
        return;
    }

    let err = ffi::nix_register_primop(ctx, primop);
    if err != ffi::NIX_OK {
        eprintln!(
            "nix-module-plugin: failed to register primop '{}': error {}",
            name, err
        );
    }
}

/// Library version for programmatic access.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
