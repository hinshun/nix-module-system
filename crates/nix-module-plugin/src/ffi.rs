//! Raw C API bindings to the Nix evaluator.
//!
//! These declarations correspond to the Nix C API (nix >= 2.18.0).
//! The plugin is loaded into the Nix process, so symbols resolve at load time.

#![allow(non_camel_case_types)]
#![allow(dead_code)]

use std::ffi::{c_char, c_double, c_int, c_uint, c_void};

// --- Opaque types ---

/// Nix C context for error reporting.
#[repr(C)]
pub struct nix_c_context {
    _opaque: [u8; 0],
}

/// Nix evaluator state.
#[repr(C)]
pub struct EvalState {
    _opaque: [u8; 0],
}

/// A Nix value handle.
#[repr(C)]
pub struct nix_value {
    _opaque: [u8; 0],
}

/// A primop handle.
#[repr(C)]
pub struct PrimOp {
    _opaque: [u8; 0],
}

/// Builder for attribute set values.
#[repr(C)]
pub struct BindingsBuilder {
    _opaque: [u8; 0],
}

/// Builder for list values.
#[repr(C)]
pub struct ListBuilder {
    _opaque: [u8; 0],
}

// --- Error codes ---

pub const NIX_OK: c_int = 0;
pub const NIX_ERR_KEY: c_int = -1;
pub const NIX_ERR_OVERFLOW: c_int = -2;
pub const NIX_ERR_UNKNOWN: c_int = -3;
pub const NIX_ERR_NIX_ERROR: c_int = -4;

pub type nix_err = c_int;

// --- Value types ---

pub const NIX_TYPE_THUNK: c_int = 0;
pub const NIX_TYPE_INT: c_int = 1;
pub const NIX_TYPE_FLOAT: c_int = 2;
pub const NIX_TYPE_BOOL: c_int = 3;
pub const NIX_TYPE_STRING: c_int = 4;
pub const NIX_TYPE_PATH: c_int = 5;
pub const NIX_TYPE_NULL: c_int = 6;
pub const NIX_TYPE_ATTRS: c_int = 7;
pub const NIX_TYPE_LIST: c_int = 8;
pub const NIX_TYPE_FUNCTION: c_int = 9;
pub const NIX_TYPE_EXTERNAL: c_int = 10;

// --- Callback types ---

/// Primop callback function signature.
pub type PrimOpFun = unsafe extern "C" fn(
    user_data: *mut c_void,
    context: *mut nix_c_context,
    state: *mut EvalState,
    args: *mut *mut nix_value,
    result: *mut nix_value,
);

/// Callback for nix_get_string (Nix >= 2.31).
pub type NixGetStringCallback =
    unsafe extern "C" fn(start: *const c_char, n: c_uint, user_data: *mut c_void);

// --- External functions ---

// --- Direct C++ ABI calls (bypassing C API EvalState wrapper) ---
//
// The C API `EvalState` struct wraps `nix::EvalState` with extra fields,
// but primop callbacks receive raw `nix::EvalState*`. Using C API functions
// that take `EvalState*` with this pointer causes segfaults. These symbols
// call nix::EvalState methods directly on the raw pointer.

extern "C" {
    /// `nix::EvalState::forceValueDeep(nix::Value& v)`
    /// Deeply forces a value (evaluates all thunks recursively).
    /// Takes the raw `nix::EvalState*` pointer (NOT the C API wrapper).
    #[link_name = "_ZN3nix9EvalState14forceValueDeepERNS_5ValueE"]
    pub fn nix_force_value_deep(state: *mut EvalState, value: *mut nix_value);
}

extern "C" {
    // Context management
    pub fn nix_c_context_create() -> *mut nix_c_context;
    pub fn nix_c_context_free(context: *mut nix_c_context);

    // Primop registration
    pub fn nix_alloc_primop(
        context: *mut nix_c_context,
        fun: PrimOpFun,
        arity: c_int,
        name: *const c_char,
        args: *mut *const c_char,
        doc: *const c_char,
        user_data: *mut c_void,
    ) -> *mut PrimOp;

    pub fn nix_register_primop(context: *mut nix_c_context, primop: *mut PrimOp) -> nix_err;

    // Value forcing
    pub fn nix_value_force(
        context: *mut nix_c_context,
        state: *mut EvalState,
        value: *mut nix_value,
    ) -> nix_err;

    // Type inspection
    pub fn nix_get_type(context: *mut nix_c_context, value: *const nix_value) -> c_int;

    // Getters (nix_get_string uses callback API since Nix 2.31)
    pub fn nix_get_string(
        context: *mut nix_c_context,
        value: *const nix_value,
        callback: NixGetStringCallback,
        user_data: *mut c_void,
    ) -> nix_err;
    pub fn nix_get_int(context: *mut nix_c_context, value: *const nix_value) -> i64;
    pub fn nix_get_float(context: *mut nix_c_context, value: *const nix_value) -> c_double;
    pub fn nix_get_bool(context: *mut nix_c_context, value: *const nix_value) -> bool;
    pub fn nix_get_path_string(
        context: *mut nix_c_context,
        value: *const nix_value,
    ) -> *const c_char;

    // List operations
    pub fn nix_get_list_size(context: *mut nix_c_context, value: *const nix_value) -> c_uint;
    pub fn nix_get_list_byidx(
        context: *mut nix_c_context,
        value: *const nix_value,
        state: *mut EvalState,
        ix: c_uint,
    ) -> *mut nix_value;

    // Attr set operations
    pub fn nix_get_attrs_size(context: *mut nix_c_context, value: *const nix_value) -> c_uint;
    pub fn nix_has_attr_byname(
        context: *mut nix_c_context,
        value: *const nix_value,
        state: *mut EvalState,
        name: *const c_char,
    ) -> bool;
    pub fn nix_get_attr_byname(
        context: *mut nix_c_context,
        value: *const nix_value,
        state: *mut EvalState,
        name: *const c_char,
    ) -> *mut nix_value;
    pub fn nix_get_attr_name_byidx(
        context: *mut nix_c_context,
        value: *const nix_value,
        state: *mut EvalState,
        i: c_uint,
    ) -> *const c_char;
    pub fn nix_get_attr_byidx(
        context: *mut nix_c_context,
        value: *const nix_value,
        state: *mut EvalState,
        i: c_uint,
        name: *mut *const c_char,
    ) -> *mut nix_value;

    // Value allocation
    pub fn nix_alloc_value(context: *mut nix_c_context, state: *mut EvalState) -> *mut nix_value;

    // Setters
    pub fn nix_init_bool(
        context: *mut nix_c_context,
        value: *mut nix_value,
        b: bool,
    ) -> nix_err;
    pub fn nix_init_int(
        context: *mut nix_c_context,
        value: *mut nix_value,
        i: i64,
    ) -> nix_err;
    pub fn nix_init_float(
        context: *mut nix_c_context,
        value: *mut nix_value,
        d: c_double,
    ) -> nix_err;
    pub fn nix_init_string(
        context: *mut nix_c_context,
        value: *mut nix_value,
        s: *const c_char,
    ) -> nix_err;
    pub fn nix_init_null(context: *mut nix_c_context, value: *mut nix_value) -> nix_err;
    pub fn nix_init_path_string(
        context: *mut nix_c_context,
        state: *mut EvalState,
        value: *mut nix_value,
        s: *const c_char,
    ) -> nix_err;

    // List builder
    pub fn nix_make_list_builder(
        context: *mut nix_c_context,
        state: *mut EvalState,
        capacity: usize,
    ) -> *mut ListBuilder;
    pub fn nix_list_builder_insert(
        context: *mut nix_c_context,
        list_builder: *mut ListBuilder,
        index: c_uint,
        value: *mut nix_value,
    ) -> nix_err;
    pub fn nix_make_list(
        context: *mut nix_c_context,
        list_builder: *mut ListBuilder,
        value: *mut nix_value,
    ) -> nix_err;
    pub fn nix_list_builder_free(list_builder: *mut ListBuilder);

    // Attr set builder
    pub fn nix_make_bindings_builder(
        context: *mut nix_c_context,
        state: *mut EvalState,
        capacity: usize,
    ) -> *mut BindingsBuilder;
    pub fn nix_bindings_builder_insert(
        context: *mut nix_c_context,
        builder: *mut BindingsBuilder,
        name: *const c_char,
        value: *mut nix_value,
    ) -> nix_err;
    pub fn nix_make_attrs(
        context: *mut nix_c_context,
        value: *mut nix_value,
        builder: *mut BindingsBuilder,
    ) -> nix_err;
    pub fn nix_bindings_builder_free(builder: *mut BindingsBuilder);
}

// --- Safe wrapper for nix_get_string callback API ---

/// Callback that copies the string data into a `Vec<u8>` pointed to by user_data.
unsafe extern "C" fn get_string_callback(
    start: *const c_char,
    n: c_uint,
    user_data: *mut c_void,
) {
    let buf = &mut *(user_data as *mut Vec<u8>);
    let slice = std::slice::from_raw_parts(start as *const u8, n as usize);
    buf.extend_from_slice(slice);
}

/// Read a Nix string value into a Rust String.
///
/// # Safety
///
/// `ctx` and `value` must be valid. `value` must be a forced string.
pub unsafe fn get_string(
    ctx: *mut nix_c_context,
    value: *const nix_value,
) -> Result<String, String> {
    let mut buf: Vec<u8> = Vec::new();
    let err = nix_get_string(
        ctx,
        value,
        get_string_callback,
        &mut buf as *mut Vec<u8> as *mut c_void,
    );
    if err != NIX_OK {
        return Err(format!("nix_get_string failed with code {}", err));
    }
    String::from_utf8(buf).map_err(|e| format!("invalid UTF-8 in string: {}", e))
}
