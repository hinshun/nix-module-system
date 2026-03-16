//! Error codes for programmatic error handling.
//!
//! Error codes provide stable identifiers for specific error conditions,
//! enabling programmatic handling and documentation.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Typed error codes for programmatic handling.
///
/// Error codes are organized by category:
/// - E0001-E0099: Type system errors
/// - E0100-E0199: Evaluation errors
/// - E0200-E0299: Module system errors
/// - E0300-E0399: Parse errors (syntax)
/// - E0400-E0499: Semantic errors (name resolution, etc.)
/// - E0500-E0599: External/FFI errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub enum ErrorCode {
    // ========== Type System Errors (E0001-E0099) ==========
    /// Type mismatch between expected and actual value
    E0001,
    /// Enum value not in allowed set
    E0002,
    /// Conflicting definitions that cannot be merged
    E0003,
    /// Option has no definition and no default
    E0004,
    /// Undefined option in a submodule
    E0005,
    /// Read-only option was modified
    E0006,
    /// Module class mismatch
    E0007,
    /// Infinite recursion detected
    E0008,
    /// Unsupported feature
    E0099,

    // ========== Evaluation Errors (E0100-E0199) ==========
    /// IO error during evaluation
    E0100,
    /// Parse error in Nix file
    E0101,
    /// Import cycle detected
    E0102,
    /// Module not found
    E0103,
    /// Import not found
    E0104,
    /// Invalid module structure
    E0105,
    /// Evaluation timeout
    E0106,
    /// Division by zero
    E0107,
    /// Index out of bounds
    E0108,
    /// Attribute not found
    E0109,
    /// Cannot coerce value
    E0110,

    // ========== Module System Errors (E0200-E0299) ==========
    /// Circular module dependency
    E0200,
    /// Module merge conflict
    E0201,
    /// Invalid module argument
    E0202,
    /// Missing required module argument
    E0203,
    /// Option already declared
    E0204,
    /// Invalid option type
    E0205,
    /// Option path collision
    E0206,
    /// Freeform module type violation
    E0207,
    /// Submodule instantiation error
    E0208,
    /// Module specialization error
    E0209,

    // ========== Parse Errors (E0300-E0399) ==========
    /// Unexpected token
    E0300,
    /// Unexpected end of file
    E0301,
    /// Invalid escape sequence
    E0302,
    /// Unterminated string
    E0303,
    /// Invalid number literal
    E0304,
    /// Invalid path literal
    E0305,
    /// Invalid identifier
    E0306,
    /// Missing semicolon
    E0307,
    /// Unbalanced brackets
    E0308,
    /// Invalid attribute path
    E0309,
    /// Invalid pattern
    E0310,
    /// Reserved keyword used as identifier
    E0311,

    // ========== Semantic Errors (E0400-E0499) ==========
    /// Undefined variable
    E0400,
    /// Variable shadowing warning
    E0401,
    /// Unused variable warning
    E0402,
    /// Unused import warning
    E0403,
    /// Assertion failure
    E0404,
    /// Throw expression
    E0405,
    /// Abort expression
    E0406,
    /// Deprecated feature
    E0407,
    /// Builtins error
    E0408,
    /// Function arity mismatch
    E0409,

    // ========== External/FFI Errors (E0500-E0599) ==========
    /// FFI call failed
    E0500,
    /// External Nix error
    E0501,
    /// Plugin load error
    E0502,
    /// Store path error
    E0503,
    /// Derivation build error
    E0504,
    /// Flake error
    E0505,

    // ========== Internal Errors (E0900-E0999) ==========
    /// Internal error (bug)
    E0900,
    /// Not implemented
    E0901,
    /// Unknown error code
    Unknown(u16),
}

impl ErrorCode {
    /// Get the numeric code as a u16.
    pub fn as_u16(&self) -> u16 {
        match self {
            // Type system errors
            ErrorCode::E0001 => 1,
            ErrorCode::E0002 => 2,
            ErrorCode::E0003 => 3,
            ErrorCode::E0004 => 4,
            ErrorCode::E0005 => 5,
            ErrorCode::E0006 => 6,
            ErrorCode::E0007 => 7,
            ErrorCode::E0008 => 8,
            ErrorCode::E0099 => 99,

            // Evaluation errors
            ErrorCode::E0100 => 100,
            ErrorCode::E0101 => 101,
            ErrorCode::E0102 => 102,
            ErrorCode::E0103 => 103,
            ErrorCode::E0104 => 104,
            ErrorCode::E0105 => 105,
            ErrorCode::E0106 => 106,
            ErrorCode::E0107 => 107,
            ErrorCode::E0108 => 108,
            ErrorCode::E0109 => 109,
            ErrorCode::E0110 => 110,

            // Module system errors
            ErrorCode::E0200 => 200,
            ErrorCode::E0201 => 201,
            ErrorCode::E0202 => 202,
            ErrorCode::E0203 => 203,
            ErrorCode::E0204 => 204,
            ErrorCode::E0205 => 205,
            ErrorCode::E0206 => 206,
            ErrorCode::E0207 => 207,
            ErrorCode::E0208 => 208,
            ErrorCode::E0209 => 209,

            // Parse errors
            ErrorCode::E0300 => 300,
            ErrorCode::E0301 => 301,
            ErrorCode::E0302 => 302,
            ErrorCode::E0303 => 303,
            ErrorCode::E0304 => 304,
            ErrorCode::E0305 => 305,
            ErrorCode::E0306 => 306,
            ErrorCode::E0307 => 307,
            ErrorCode::E0308 => 308,
            ErrorCode::E0309 => 309,
            ErrorCode::E0310 => 310,
            ErrorCode::E0311 => 311,

            // Semantic errors
            ErrorCode::E0400 => 400,
            ErrorCode::E0401 => 401,
            ErrorCode::E0402 => 402,
            ErrorCode::E0403 => 403,
            ErrorCode::E0404 => 404,
            ErrorCode::E0405 => 405,
            ErrorCode::E0406 => 406,
            ErrorCode::E0407 => 407,
            ErrorCode::E0408 => 408,
            ErrorCode::E0409 => 409,

            // External errors
            ErrorCode::E0500 => 500,
            ErrorCode::E0501 => 501,
            ErrorCode::E0502 => 502,
            ErrorCode::E0503 => 503,
            ErrorCode::E0504 => 504,
            ErrorCode::E0505 => 505,

            // Internal errors
            ErrorCode::E0900 => 900,
            ErrorCode::E0901 => 901,

            ErrorCode::Unknown(n) => *n,
        }
    }

    /// Create an error code from a numeric value.
    pub fn from_u16(n: u16) -> Self {
        match n {
            // Type system errors
            1 => ErrorCode::E0001,
            2 => ErrorCode::E0002,
            3 => ErrorCode::E0003,
            4 => ErrorCode::E0004,
            5 => ErrorCode::E0005,
            6 => ErrorCode::E0006,
            7 => ErrorCode::E0007,
            8 => ErrorCode::E0008,
            99 => ErrorCode::E0099,

            // Evaluation errors
            100 => ErrorCode::E0100,
            101 => ErrorCode::E0101,
            102 => ErrorCode::E0102,
            103 => ErrorCode::E0103,
            104 => ErrorCode::E0104,
            105 => ErrorCode::E0105,
            106 => ErrorCode::E0106,
            107 => ErrorCode::E0107,
            108 => ErrorCode::E0108,
            109 => ErrorCode::E0109,
            110 => ErrorCode::E0110,

            // Module system errors
            200 => ErrorCode::E0200,
            201 => ErrorCode::E0201,
            202 => ErrorCode::E0202,
            203 => ErrorCode::E0203,
            204 => ErrorCode::E0204,
            205 => ErrorCode::E0205,
            206 => ErrorCode::E0206,
            207 => ErrorCode::E0207,
            208 => ErrorCode::E0208,
            209 => ErrorCode::E0209,

            // Parse errors
            300 => ErrorCode::E0300,
            301 => ErrorCode::E0301,
            302 => ErrorCode::E0302,
            303 => ErrorCode::E0303,
            304 => ErrorCode::E0304,
            305 => ErrorCode::E0305,
            306 => ErrorCode::E0306,
            307 => ErrorCode::E0307,
            308 => ErrorCode::E0308,
            309 => ErrorCode::E0309,
            310 => ErrorCode::E0310,
            311 => ErrorCode::E0311,

            // Semantic errors
            400 => ErrorCode::E0400,
            401 => ErrorCode::E0401,
            402 => ErrorCode::E0402,
            403 => ErrorCode::E0403,
            404 => ErrorCode::E0404,
            405 => ErrorCode::E0405,
            406 => ErrorCode::E0406,
            407 => ErrorCode::E0407,
            408 => ErrorCode::E0408,
            409 => ErrorCode::E0409,

            // External errors
            500 => ErrorCode::E0500,
            501 => ErrorCode::E0501,
            502 => ErrorCode::E0502,
            503 => ErrorCode::E0503,
            504 => ErrorCode::E0504,
            505 => ErrorCode::E0505,

            // Internal errors
            900 => ErrorCode::E0900,
            901 => ErrorCode::E0901,

            n => ErrorCode::Unknown(n),
        }
    }

    /// Get a human-readable description of the error code.
    pub fn description(&self) -> &'static str {
        match self {
            // Type system errors
            ErrorCode::E0001 => "type mismatch",
            ErrorCode::E0002 => "invalid enum value",
            ErrorCode::E0003 => "conflicting definitions",
            ErrorCode::E0004 => "missing definition",
            ErrorCode::E0005 => "undefined option",
            ErrorCode::E0006 => "read-only violation",
            ErrorCode::E0007 => "module class mismatch",
            ErrorCode::E0008 => "infinite recursion",
            ErrorCode::E0099 => "unsupported feature",

            // Evaluation errors
            ErrorCode::E0100 => "IO error",
            ErrorCode::E0101 => "parse error",
            ErrorCode::E0102 => "import cycle",
            ErrorCode::E0103 => "module not found",
            ErrorCode::E0104 => "import not found",
            ErrorCode::E0105 => "invalid module",
            ErrorCode::E0106 => "timeout",
            ErrorCode::E0107 => "division by zero",
            ErrorCode::E0108 => "index out of bounds",
            ErrorCode::E0109 => "attribute not found",
            ErrorCode::E0110 => "coercion error",

            // Module system errors
            ErrorCode::E0200 => "circular dependency",
            ErrorCode::E0201 => "merge conflict",
            ErrorCode::E0202 => "invalid module argument",
            ErrorCode::E0203 => "missing required argument",
            ErrorCode::E0204 => "option already declared",
            ErrorCode::E0205 => "invalid option type",
            ErrorCode::E0206 => "option path collision",
            ErrorCode::E0207 => "freeform type violation",
            ErrorCode::E0208 => "submodule error",
            ErrorCode::E0209 => "specialization error",

            // Parse errors
            ErrorCode::E0300 => "unexpected token",
            ErrorCode::E0301 => "unexpected end of file",
            ErrorCode::E0302 => "invalid escape sequence",
            ErrorCode::E0303 => "unterminated string",
            ErrorCode::E0304 => "invalid number",
            ErrorCode::E0305 => "invalid path",
            ErrorCode::E0306 => "invalid identifier",
            ErrorCode::E0307 => "missing semicolon",
            ErrorCode::E0308 => "unbalanced brackets",
            ErrorCode::E0309 => "invalid attribute path",
            ErrorCode::E0310 => "invalid pattern",
            ErrorCode::E0311 => "reserved keyword",

            // Semantic errors
            ErrorCode::E0400 => "undefined variable",
            ErrorCode::E0401 => "variable shadowing",
            ErrorCode::E0402 => "unused variable",
            ErrorCode::E0403 => "unused import",
            ErrorCode::E0404 => "assertion failure",
            ErrorCode::E0405 => "throw expression",
            ErrorCode::E0406 => "abort expression",
            ErrorCode::E0407 => "deprecated feature",
            ErrorCode::E0408 => "builtins error",
            ErrorCode::E0409 => "arity mismatch",

            // External errors
            ErrorCode::E0500 => "FFI error",
            ErrorCode::E0501 => "external Nix error",
            ErrorCode::E0502 => "plugin error",
            ErrorCode::E0503 => "store path error",
            ErrorCode::E0504 => "build error",
            ErrorCode::E0505 => "flake error",

            // Internal errors
            ErrorCode::E0900 => "internal error",
            ErrorCode::E0901 => "not implemented",

            ErrorCode::Unknown(_) => "unknown error",
        }
    }

    /// Get the category of this error code.
    pub fn category(&self) -> ErrorCategory {
        let n = self.as_u16();
        match n {
            0..=99 => ErrorCategory::TypeSystem,
            100..=199 => ErrorCategory::Evaluation,
            200..=299 => ErrorCategory::ModuleSystem,
            300..=399 => ErrorCategory::Parse,
            400..=499 => ErrorCategory::Semantic,
            500..=599 => ErrorCategory::External,
            900..=999 => ErrorCategory::Internal,
            _ => ErrorCategory::Unknown,
        }
    }

    /// Check if this is a warning-level code.
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            ErrorCode::E0401 | // shadowing
            ErrorCode::E0402 | // unused variable
            ErrorCode::E0403 | // unused import
            ErrorCode::E0407   // deprecated
        )
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E{:04}", self.as_u16())
    }
}

impl From<ErrorCode> for String {
    fn from(code: ErrorCode) -> String {
        code.to_string()
    }
}

impl TryFrom<String> for ErrorCode {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        if !s.starts_with('E') {
            return Err(format!("Invalid error code format: {}", s));
        }

        let num_str = &s[1..];
        let num: u16 = num_str
            .parse()
            .map_err(|_| format!("Invalid error code number: {}", num_str))?;

        Ok(ErrorCode::from_u16(num))
    }
}

/// Category of error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Type system errors (E0001-E0099)
    TypeSystem,
    /// Evaluation errors (E0100-E0199)
    Evaluation,
    /// Module system errors (E0200-E0299)
    ModuleSystem,
    /// Parse errors (E0300-E0399)
    Parse,
    /// Semantic errors (E0400-E0499)
    Semantic,
    /// External/FFI errors (E0500-E0599)
    External,
    /// Internal errors (E0900-E0999)
    Internal,
    /// Unknown category
    Unknown,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCategory::TypeSystem => write!(f, "type system"),
            ErrorCategory::Evaluation => write!(f, "evaluation"),
            ErrorCategory::ModuleSystem => write!(f, "module system"),
            ErrorCategory::Parse => write!(f, "parse"),
            ErrorCategory::Semantic => write!(f, "semantic"),
            ErrorCategory::External => write!(f, "external"),
            ErrorCategory::Internal => write!(f, "internal"),
            ErrorCategory::Unknown => write!(f, "unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_display() {
        assert_eq!(format!("{}", ErrorCode::E0001), "E0001");
        assert_eq!(format!("{}", ErrorCode::E0100), "E0100");
        assert_eq!(format!("{}", ErrorCode::E0300), "E0300");
    }

    #[test]
    fn test_error_code_roundtrip() {
        for code in [
            ErrorCode::E0001,
            ErrorCode::E0005,
            ErrorCode::E0100,
            ErrorCode::E0300,
            ErrorCode::E0400,
        ] {
            let n = code.as_u16();
            assert_eq!(ErrorCode::from_u16(n), code);
        }
    }

    #[test]
    fn test_error_code_category() {
        assert_eq!(ErrorCode::E0001.category(), ErrorCategory::TypeSystem);
        assert_eq!(ErrorCode::E0100.category(), ErrorCategory::Evaluation);
        assert_eq!(ErrorCode::E0200.category(), ErrorCategory::ModuleSystem);
        assert_eq!(ErrorCode::E0300.category(), ErrorCategory::Parse);
        assert_eq!(ErrorCode::E0400.category(), ErrorCategory::Semantic);
        assert_eq!(ErrorCode::E0500.category(), ErrorCategory::External);
        assert_eq!(ErrorCode::E0900.category(), ErrorCategory::Internal);
    }

    #[test]
    fn test_error_code_description() {
        assert_eq!(ErrorCode::E0001.description(), "type mismatch");
        assert_eq!(ErrorCode::E0102.description(), "import cycle");
        assert_eq!(ErrorCode::E0300.description(), "unexpected token");
        assert_eq!(ErrorCode::E0400.description(), "undefined variable");
    }

    #[test]
    fn test_error_code_serialization() {
        let code = ErrorCode::E0005;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"E0005\"");

        let deserialized: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(code, deserialized);
    }

    #[test]
    fn test_error_code_from_string() {
        assert_eq!(
            ErrorCode::try_from("E0001".to_string()),
            Ok(ErrorCode::E0001)
        );
        assert_eq!(
            ErrorCode::try_from("E0300".to_string()),
            Ok(ErrorCode::E0300)
        );
        assert!(ErrorCode::try_from("invalid".to_string()).is_err());
        assert!(ErrorCode::try_from("0001".to_string()).is_err());
    }

    #[test]
    fn test_is_warning() {
        assert!(!ErrorCode::E0001.is_warning());
        assert!(ErrorCode::E0401.is_warning());
        assert!(ErrorCode::E0402.is_warning());
        assert!(ErrorCode::E0407.is_warning());
    }

    #[test]
    fn test_unknown_code() {
        let unknown = ErrorCode::Unknown(9999);
        assert_eq!(unknown.as_u16(), 9999);
        assert_eq!(format!("{}", unknown), "E9999");
        assert_eq!(unknown.description(), "unknown error");
    }
}
