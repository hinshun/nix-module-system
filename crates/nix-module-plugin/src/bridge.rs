//! Bridge between Nix C API values and Rust `Value` types.
//!
//! With the JSON-based primop protocol, most bridge functions are unused.
//! Primops receive and return JSON strings — the Nix side handles
//! serialization via `builtins.toJSON` / `builtins.fromJSON`.
//!
//! The remaining bridge code is kept for potential future use (e.g., if we
//! solve the EvalState wrapper mismatch and can access attrsets directly).

// NOTE: This module is currently unused since primops use JSON strings.
// The code is preserved for reference and potential future direct C API usage.
