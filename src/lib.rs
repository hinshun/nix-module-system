//! # Nix Module System v2
//!
//! A high-performance Nix module system implementation with:
//! - Rust-based type checking and merging
//! - No fixed-point evaluation
//! - Beautiful error messages via ariadne
//! - LSP support
//!
//! ## Architecture
//!
//! This crate provides Nix primops implemented in Rust for performance.
//! The evaluation uses a staged pipeline instead of fixed-point iteration:
//!
//! 1. **Parse**: Extract module structure (imports, options, config)
//! 2. **Collect**: Resolve imports and build dependency graph
//! 3. **Declare**: Process option declarations (no config access needed)
//! 4. **Define**: Merge config definitions using lattice unification
//!
//! ## Usage
//!
//! ### As a Library (Recommended)
//!
//! ```ignore
//! use nix_module_system::api::{ModuleEvaluator, EvaluatedConfig};
//!
//! // Simple usage
//! let config = ModuleEvaluator::new()
//!     .add_file("./configuration.nix")?
//!     .add_file("./hardware.nix")?
//!     .evaluate()?;
//!
//! let nginx_port: i64 = config.get("services.nginx.port")?;
//! let enabled: bool = config.get("services.nginx.enable")?;
//!
//! // With options introspection
//! for option in config.options() {
//!     println!("{}: {} = {:?}", option.path, option.type_desc, option.default);
//! }
//!
//! // With error streaming
//! let config = ModuleEvaluator::new()
//!     .add_file("./config.nix")?
//!     .on_diagnostic(|diag| eprintln!("{}", diag.message))
//!     .evaluate()?;
//! ```
//!
//! ### As a Nix Plugin
//!
//! ```bash
//! nix eval --plugin-files ./libnix_module_system.so --expr '...'
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod api;
pub mod docs;
pub mod errors;
pub mod eval;
pub mod ffi;
pub mod merge;
pub mod nix;
pub mod parse;
pub mod types;

#[cfg(feature = "lsp")]
pub mod lsp;

use std::ffi::{c_char, c_int, c_void, CStr};
use std::panic;

/// Version of the module system
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Opaque handle to Nix EvalState
#[repr(C)]
pub struct EvalState {
    _private: [u8; 0],
}

/// Opaque handle to Nix Value
#[repr(C)]
pub struct NixValue {
    _private: [u8; 0],
}

/// Opaque handle to Nix context for error reporting
#[repr(C)]
pub struct NixContext {
    _private: [u8; 0],
}

/// Result type for FFI operations
pub type FfiResult = c_int;

/// FFI success code
pub const FFI_OK: FfiResult = 0;
/// FFI error code
pub const FFI_ERROR: FfiResult = -1;
/// FFI type error code
pub const FFI_TYPE_ERROR: FfiResult = -2;
/// FFI not implemented error code
pub const FFI_NOT_IMPLEMENTED: FfiResult = -3;

/// Check if a value matches a type.
///
/// # Safety
///
/// - `ctx` must be a valid Nix context or null
/// - `type_name` must be a valid null-terminated C string
/// - `value` must be a valid Nix value pointer
///
/// # Returns
///
/// - `1` if the value matches the type
/// - `0` if it doesn't match
/// - `-1` on error
#[no_mangle]
pub unsafe extern "C" fn nms_check_type(
    _ctx: *mut NixContext,
    _state: *mut EvalState,
    type_name: *const c_char,
    _value: *mut NixValue,
) -> FfiResult {
    panic::catch_unwind(|| {
        if type_name.is_null() {
            return FFI_ERROR;
        }

        let type_name = match CStr::from_ptr(type_name).to_str() {
            Ok(s) => s,
            Err(_) => return FFI_ERROR,
        };

        // FFI type checking requires nix-ffi feature to be implemented
        tracing::debug!("nms_check_type called with type: {} (not implemented)", type_name);
        FFI_NOT_IMPLEMENTED
    })
    .unwrap_or(FFI_ERROR)
}

/// Merge multiple definitions according to a type's merge strategy.
///
/// # Safety
///
/// - `ctx` must be a valid Nix context or null
/// - `state` must be a valid EvalState pointer
/// - `type_ptr` must be a valid type handle from nms_create_type
/// - `defs` must be a valid Nix list of definitions
/// - `result` must be a valid pointer for writing the result
#[no_mangle]
pub unsafe extern "C" fn nms_merge_definitions(
    _ctx: *mut NixContext,
    _state: *mut EvalState,
    _type_ptr: *mut c_void,
    _defs: *mut NixValue,
    _result: *mut NixValue,
) -> FfiResult {
    panic::catch_unwind(|| {
        // FFI merge requires nix-ffi feature to be implemented
        tracing::debug!("nms_merge_definitions called (not implemented)");
        FFI_NOT_IMPLEMENTED
    })
    .unwrap_or(FFI_ERROR)
}

/// Evaluate modules using the staged pipeline.
///
/// # Safety
///
/// - `ctx` must be a valid Nix context or null
/// - `state` must be a valid EvalState pointer
/// - `modules` must be a valid Nix list of modules
/// - `result` must be a valid pointer for writing the result
#[no_mangle]
pub unsafe extern "C" fn nms_eval_modules(
    _ctx: *mut NixContext,
    _state: *mut EvalState,
    _modules: *mut NixValue,
    _result: *mut NixValue,
) -> FfiResult {
    panic::catch_unwind(|| {
        // FFI evalModules requires nix-ffi feature to be implemented
        tracing::debug!("nms_eval_modules called (not implemented)");
        FFI_NOT_IMPLEMENTED
    })
    .unwrap_or(FFI_ERROR)
}

/// Get the last error message.
///
/// # Safety
///
/// Returns a pointer to a static string. Do not free.
/// The string is valid until the next FFI call from the same thread.
/// Static message for unimplemented error retrieval
static NOT_IMPLEMENTED_MSG: &[u8] = b"Error retrieval not implemented\0";

#[no_mangle]
pub unsafe extern "C" fn nms_get_error() -> *const c_char {
    // Returns a static message indicating error storage is not implemented
    // Full implementation would use thread-local storage
    NOT_IMPLEMENTED_MSG.as_ptr() as *const c_char
}

/// Free a string allocated by this library.
///
/// # Safety
///
/// - `s` must be a pointer returned by an nms_* function
/// - Must only be called once per string
#[no_mangle]
pub unsafe extern "C" fn nms_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(std::ffi::CString::from_raw(s));
    }
}

/// Null-terminated version string for C FFI
const VERSION_CSTR: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

/// Get the library version.
///
/// # Safety
///
/// Returns a pointer to a static null-terminated string. Do not free.
#[no_mangle]
pub extern "C" fn nms_version() -> *const c_char {
    VERSION_CSTR.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let version = unsafe { CStr::from_ptr(nms_version()) };
        let version_str = version.to_str().unwrap();
        assert!(!version_str.is_empty());
        assert_eq!(version_str, VERSION);
    }
}
