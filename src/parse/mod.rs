//! Parser for Nix expressions.
//!
//! This module provides a recursive descent parser for Nix source code,
//! producing an AST with span information for error reporting.
//!
//! ## Module Structure
//!
//! Nix modules follow the pattern:
//! ```nix
//! { config, lib, pkgs, ... }: {
//!   options = { ... };
//!   config = { ... };
//! }
//! ```
//!
//! ## Supported Constructs
//!
//! - Attribute sets (recursive and non-recursive)
//! - Lambda functions (simple and pattern matching)
//! - Let expressions
//! - If expressions
//! - With expressions
//! - Function application
//! - Attribute selection
//! - Module helpers: mkOption, mkIf, mkDefault, mkForce, mkMerge, mkOverride

pub mod lexer;
pub mod parser;

pub use lexer::{Lexer, Token, TokenKind};
pub use parser::{ParseError, Parser};

use crate::types::Span;
use std::path::PathBuf;

/// A spanned AST node
#[derive(Debug, Clone)]
pub struct Spanned<T> {
    /// The node value
    pub node: T,
    /// Source location
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Create a new spanned node
    pub fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

/// Nix expression AST
#[derive(Debug, Clone)]
pub enum Expr {
    /// Identifier (e.g., `config`, `lib`)
    Ident(String),

    /// Null literal
    Null,

    /// Boolean literal
    Bool(bool),

    /// Integer literal
    Int(i64),

    /// Float literal
    Float(f64),

    /// String literal (possibly with interpolations)
    String(StringParts),

    /// Path literal
    Path(PathBuf),

    /// List literal
    List(Vec<Spanned<Expr>>),

    /// Attribute set
    AttrSet(AttrSet),

    /// Let expression
    Let {
        /// Bindings
        bindings: Vec<Binding>,
        /// Body expression
        body: Box<Spanned<Expr>>,
    },

    /// If expression
    If {
        /// Condition
        cond: Box<Spanned<Expr>>,
        /// Then branch
        then_expr: Box<Spanned<Expr>>,
        /// Else branch
        else_expr: Box<Spanned<Expr>>,
    },

    /// With expression
    With {
        /// Environment expression
        env: Box<Spanned<Expr>>,
        /// Body expression
        body: Box<Spanned<Expr>>,
    },

    /// Assert expression
    Assert {
        /// Assertion condition
        cond: Box<Spanned<Expr>>,
        /// Body expression
        body: Box<Spanned<Expr>>,
    },

    /// Lambda function
    Lambda(Lambda),

    /// Function application
    Apply {
        /// Function expression
        func: Box<Spanned<Expr>>,
        /// Argument expression
        arg: Box<Spanned<Expr>>,
    },

    /// Attribute selection (e.g., `x.y.z`)
    Select {
        /// Base expression
        expr: Box<Spanned<Expr>>,
        /// Attribute path
        path: Vec<Spanned<AttrName>>,
        /// Default value (for `x.y or default`)
        default: Option<Box<Spanned<Expr>>>,
    },

    /// Has attribute (e.g., `x ? y`)
    HasAttr {
        /// Base expression
        expr: Box<Spanned<Expr>>,
        /// Attribute path
        path: Vec<Spanned<AttrName>>,
    },

    /// Binary operation
    BinOp {
        /// Left operand
        left: Box<Spanned<Expr>>,
        /// Operator
        op: BinOp,
        /// Right operand
        right: Box<Spanned<Expr>>,
    },

    /// Unary operation
    UnaryOp {
        /// Operator
        op: UnaryOp,
        /// Operand
        expr: Box<Spanned<Expr>>,
    },

    /// Error recovery placeholder
    Error,
}

/// String with possible interpolations
#[derive(Debug, Clone)]
pub struct StringParts {
    /// String parts
    pub parts: Vec<StringPart>,
}

impl StringParts {
    /// Create a simple string with no interpolations
    pub fn simple(s: String) -> Self {
        Self {
            parts: vec![StringPart::Literal(s)],
        }
    }

    /// Check if this is a simple string (no interpolations)
    pub fn is_simple(&self) -> bool {
        self.parts.len() == 1 && matches!(&self.parts[0], StringPart::Literal(_))
    }

    /// Get the string value if simple
    pub fn as_simple(&self) -> Option<&str> {
        if self.is_simple() {
            if let StringPart::Literal(s) = &self.parts[0] {
                return Some(s);
            }
        }
        None
    }
}

/// Part of a string (literal or interpolation)
#[derive(Debug, Clone)]
pub enum StringPart {
    /// Literal string
    Literal(String),
    /// Interpolated expression
    Interpolation(Box<Spanned<Expr>>),
}

/// Attribute set
#[derive(Debug, Clone)]
pub struct AttrSet {
    /// Whether the attribute set is recursive
    pub recursive: bool,
    /// Bindings in the attribute set
    pub bindings: Vec<Binding>,
}

impl AttrSet {
    /// Create an empty non-recursive attribute set
    pub fn empty() -> Self {
        Self {
            recursive: false,
            bindings: Vec::new(),
        }
    }
}

/// Attribute name (identifier, string, or interpolation)
#[derive(Debug, Clone)]
pub enum AttrName {
    /// Simple identifier
    Ident(String),
    /// String (possibly dynamic)
    String(StringParts),
    /// Interpolated expression
    Interpolation(Box<Spanned<Expr>>),
}

/// Binding in an attribute set or let expression
#[derive(Debug, Clone)]
pub enum Binding {
    /// Simple binding: `name = value;`
    Simple {
        /// Attribute path
        path: Vec<Spanned<AttrName>>,
        /// Value expression
        value: Spanned<Expr>,
    },
    /// Inherit binding: `inherit x y z;` or `inherit (expr) x y z;`
    Inherit {
        /// Source expression (if any)
        from: Option<Spanned<Expr>>,
        /// Inherited names
        names: Vec<Spanned<String>>,
    },
}

/// Lambda function
#[derive(Debug, Clone)]
pub struct Lambda {
    /// Parameter pattern
    pub param: LambdaParam,
    /// Function body
    pub body: Box<Spanned<Expr>>,
}

/// Lambda parameter pattern
#[derive(Debug, Clone)]
pub enum LambdaParam {
    /// Simple identifier parameter
    Ident(String),

    /// Pattern matching on attribute set
    Pattern {
        /// Pattern entries
        entries: Vec<PatternEntry>,
        /// Whether the pattern accepts extra attributes (has `...`)
        ellipsis: bool,
        /// Optional name binding for the whole argument
        at: Option<String>,
    },
}

/// Entry in a pattern
#[derive(Debug, Clone)]
pub struct PatternEntry {
    /// Parameter name
    pub name: Spanned<String>,
    /// Default value (if any)
    pub default: Option<Spanned<Expr>>,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `++`
    Concat,
    /// `//`
    Update,
    /// `==`
    Eq,
    /// `!=`
    NotEq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    LtEq,
    /// `>=`
    GtEq,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `->`
    Implication,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-` (negation)
    Neg,
    /// `!` (logical not)
    Not,
}

/// Parse a Nix expression from source
pub fn parse(source: &str, file: PathBuf) -> Result<Spanned<Expr>, Vec<ParseError>> {
    let mut parser = Parser::new(source, file);
    parser.parse_expr()
}

/// Parse a Nix module from source
pub fn parse_module(source: &str, file: PathBuf) -> Result<Spanned<Expr>, Vec<ParseError>> {
    let mut parser = Parser::new(source, file);
    parser.parse_module()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_attrset() {
        let source = "{ x = 1; y = 2; }";
        let result = parse(source, PathBuf::from("test.nix"));
        assert!(result.is_ok());

        if let Ok(Spanned {
            node: Expr::AttrSet(attrs),
            ..
        }) = result
        {
            assert_eq!(attrs.bindings.len(), 2);
            assert!(!attrs.recursive);
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_lambda() {
        let source = "x: x + 1";
        let result = parse(source, PathBuf::from("test.nix"));
        assert!(result.is_ok());

        if let Ok(Spanned {
            node: Expr::Lambda(lambda),
            ..
        }) = result
        {
            assert!(matches!(lambda.param, LambdaParam::Ident(ref s) if s == "x"));
        } else {
            panic!("Expected Lambda");
        }
    }

    #[test]
    fn test_parse_module_structure() {
        let source = r#"{ config, lib, ... }: {
            options = {};
            config = {};
        }"#;
        let result = parse_module(source, PathBuf::from("test.nix"));
        assert!(result.is_ok());

        if let Ok(Spanned {
            node: Expr::Lambda(lambda),
            ..
        }) = result
        {
            if let LambdaParam::Pattern {
                entries, ellipsis, ..
            } = lambda.param
            {
                assert_eq!(entries.len(), 2);
                assert!(ellipsis);
            } else {
                panic!("Expected Pattern");
            }
        } else {
            panic!("Expected Lambda");
        }
    }

    #[test]
    fn test_parse_mkoption() {
        let source = r#"mkOption {
            type = types.bool;
            default = false;
            description = "Enable the service";
        }"#;
        let result = parse(source, PathBuf::from("test.nix"));
        assert!(result.is_ok());

        if let Ok(Spanned {
            node: Expr::Apply { func, arg },
            ..
        }) = result
        {
            if let Expr::Ident(name) = &func.node {
                assert_eq!(name, "mkOption");
            } else {
                panic!("Expected Ident for func");
            }

            if let Expr::AttrSet(attrs) = &arg.node {
                assert_eq!(attrs.bindings.len(), 3);
            } else {
                panic!("Expected AttrSet for arg");
            }
        } else {
            panic!("Expected Apply");
        }
    }
}
