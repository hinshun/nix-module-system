//! Recursive descent parser for Nix expressions.
//!
//! This parser produces an AST with span information for error reporting.
//! It supports error recovery for incomplete expressions.

use super::lexer::{Lexer, Token, TokenKind};
use super::{
    AttrName, AttrSet, BinOp, Binding, Expr, Lambda, LambdaParam, PatternEntry, Spanned,
    StringParts, UnaryOp,
};
use crate::types::Span;
use std::path::PathBuf;

/// Parse error
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Error message
    pub message: String,
    /// Source location
    pub span: Span,
    /// Hints for recovery
    pub hints: Vec<String>,
}

impl ParseError {
    /// Create a new parse error
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            hints: Vec::new(),
        }
    }

    /// Add a hint to the error
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parser for Nix expressions
pub struct Parser {
    /// Lexer for tokenization
    lexer: Lexer,
    /// Original source code
    source: String,
    /// Current token
    current: Token,
    /// Previous token (for spans)
    previous: Token,
    /// Accumulated errors
    errors: Vec<ParseError>,
    /// Panic mode for error recovery
    panic_mode: bool,
}

impl Parser {
    /// Create a new parser for the given source
    pub fn new(source: &str, file: PathBuf) -> Self {
        let mut lexer = Lexer::new(source, file.clone());
        let current = lexer.next_token();
        let previous = Token::new(
            TokenKind::Eof,
            Span::new(file, 0, 0, 1, 1),
        );

        Self {
            lexer,
            source: source.to_string(),
            current,
            previous,
            errors: Vec::new(),
            panic_mode: false,
        }
    }

    /// Advance to the next token
    fn advance(&mut self) {
        self.previous = std::mem::replace(&mut self.current, self.lexer.next_token());

        // Skip error tokens and collect errors
        while let TokenKind::Error(ref msg) = self.current.kind {
            let error = ParseError::new(msg.clone(), self.current.span.clone());
            self.errors.push(error);
            self.current = self.lexer.next_token();
        }
    }

    /// Check if current token matches the expected kind
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current.kind) == std::mem::discriminant(kind)
    }

    /// Consume the current token if it matches, otherwise return error
    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.check(&kind) {
            let token = self.current.clone();
            self.advance();
            Ok(token)
        } else {
            Err(ParseError::new(
                format!("Expected {:?}, found {:?}", kind, self.current.kind),
                self.current.span.clone(),
            ))
        }
    }

    /// Consume the current token if it matches
    fn consume(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Report an error and enter panic mode
    fn error(&mut self, message: impl Into<String>) {
        if self.panic_mode {
            return;
        }
        self.panic_mode = true;
        self.errors.push(ParseError::new(message, self.current.span.clone()));
    }

    /// Create a span from start position to current position
    fn span_from(&self, start: &Span) -> Span {
        Span::new(
            start.file.clone(),
            start.start,
            self.previous.span.end,
            start.line,
            start.column,
        )
    }

    /// Synchronize after an error
    fn synchronize(&mut self) {
        self.panic_mode = false;

        while !matches!(self.current.kind, TokenKind::Eof) {
            // Stop at statement boundaries
            if matches!(self.previous.kind, TokenKind::Semicolon) {
                return;
            }

            // Stop at certain keywords/tokens that likely start new constructs
            match self.current.kind {
                TokenKind::Let
                | TokenKind::If
                | TokenKind::With
                | TokenKind::Assert
                | TokenKind::RBrace
                | TokenKind::RBracket
                | TokenKind::RParen => return,
                _ => self.advance(),
            }
        }
    }

    /// Parse a complete expression
    pub fn parse_expr(&mut self) -> Result<Spanned<Expr>, Vec<ParseError>> {
        let expr = self.expr();

        if !self.errors.is_empty() {
            return Err(std::mem::take(&mut self.errors));
        }

        expr.ok_or_else(|| vec![ParseError::new("Failed to parse expression", self.current.span.clone())])
    }

    /// Parse a module (lambda with pattern parameter)
    pub fn parse_module(&mut self) -> Result<Spanned<Expr>, Vec<ParseError>> {
        self.parse_expr()
    }

    /// Parse an expression
    fn expr(&mut self) -> Option<Spanned<Expr>> {
        self.expr_function()
    }

    /// Parse function expression (lambda or implication)
    fn expr_function(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();

        // Check for lambda patterns
        // { ... }: body
        // ident: body
        // { ... } @ ident: body
        // ident @ { ... }: body

        // Try to parse a pattern or identifier
        match &self.current.kind {
            TokenKind::LBrace => {
                // Could be lambda pattern or attr set
                // Look ahead to determine
                if self.is_lambda_pattern() {
                    return self.parse_lambda_with_pattern(start);
                }
            }
            TokenKind::Ident(_) => {
                // Could be simple lambda, @-pattern, or just an expression
                if self.is_lambda() {
                    return self.parse_lambda(start);
                }
            }
            _ => {}
        }

        // Not a lambda, parse as operation
        self.expr_op()
    }

    /// Check if current position is a lambda pattern
    /// A lambda pattern is { ... }: or { ... } @
    /// An attr set is { ... = ...; } or { inherit ...; } or rec { }
    fn is_lambda_pattern(&self) -> bool {
        if !matches!(self.current.kind, TokenKind::LBrace) {
            return false;
        }

        // Scan ahead to distinguish:
        // Lambda: { x, y, ... }: or { } @ name:
        // AttrSet: { x = ...; } or { inherit ...; }

        // Create a temporary lexer to look ahead
        // This is inefficient but correct
        let mut temp_lexer = Lexer::new(
            &self.source_from_current(),
            self.current.span.file.clone(),
        );

        // Skip the opening brace
        temp_lexer.next_token();

        // Check what follows
        let first = temp_lexer.next_token();

        match &first.kind {
            // Empty braces followed by : or @ means lambda
            TokenKind::RBrace => {
                let next = temp_lexer.next_token();
                matches!(next.kind, TokenKind::Colon | TokenKind::At)
            }
            // Ellipsis inside braces means lambda pattern
            TokenKind::Ellipsis => true,
            // identifier followed by comma, ?, or } (at end) means lambda
            TokenKind::Ident(_) => {
                let next = temp_lexer.next_token();
                matches!(
                    next.kind,
                    TokenKind::Comma | TokenKind::Question | TokenKind::RBrace
                )
            }
            // inherit means attr set
            TokenKind::Inherit => false,
            // Anything else is probably an attr set
            _ => false,
        }
    }

    /// Check if current identifier starts a lambda
    fn is_lambda(&self) -> bool {
        // Look for ident: or ident @
        if let TokenKind::Ident(_) = &self.current.kind {
            // Create a temporary lexer to peek ahead
            let mut temp_lexer = Lexer::new(
                &self.source_from_current(),
                self.current.span.file.clone(),
            );

            // Skip the identifier
            temp_lexer.next_token();

            // Check what follows
            let next = temp_lexer.next_token();
            matches!(next.kind, TokenKind::Colon | TokenKind::At)
        } else {
            false
        }
    }

    /// Get the remaining source from current position
    fn source_from_current(&self) -> String {
        self.source[self.current.span.start..].to_string()
    }

    /// Parse lambda with pattern parameter
    fn parse_lambda_with_pattern(&mut self, start: Span) -> Option<Spanned<Expr>> {
        // { param1, param2, ... }: body
        self.advance(); // consume {

        let mut entries = Vec::new();
        let mut ellipsis = false;
        let mut at_name = None;

        loop {
            match &self.current.kind {
                TokenKind::Ellipsis => {
                    ellipsis = true;
                    self.advance();
                }
                TokenKind::Ident(name) => {
                    let name_span = self.current.span.clone();
                    let name = name.clone();
                    self.advance();

                    // Check for default value
                    let default = if self.consume(&TokenKind::Question) {
                        self.expr()
                    } else {
                        None
                    };

                    entries.push(PatternEntry {
                        name: Spanned::new(name, name_span),
                        default,
                    });
                }
                TokenKind::RBrace => break,
                TokenKind::Comma => {
                    self.advance();
                    continue;
                }
                _ => {
                    self.error(format!("Unexpected token in pattern: {:?}", self.current.kind));
                    self.synchronize();
                    return None;
                }
            }

            // After each element, expect comma or closing brace
            if !self.consume(&TokenKind::Comma) {
                break;
            }
        }

        if self.expect(TokenKind::RBrace).is_err() {
            self.synchronize();
            return None;
        }

        // Check for @ pattern
        if self.consume(&TokenKind::At) {
            if let TokenKind::Ident(name) = &self.current.kind {
                at_name = Some(name.clone());
                self.advance();
            } else {
                self.error("Expected identifier after @");
                self.synchronize();
                return None;
            }
        }

        // Expect colon
        if self.expect(TokenKind::Colon).is_err() {
            // This might not be a lambda after all - synchronize and return None
            // Future enhancement: could attempt to parse as a different expression type
            self.synchronize();
            return None;
        }

        // Parse body
        let body = self.expr()?;

        let lambda = Lambda {
            param: LambdaParam::Pattern {
                entries,
                ellipsis,
                at: at_name,
            },
            body: Box::new(body),
        };

        Some(Spanned::new(Expr::Lambda(lambda), self.span_from(&start)))
    }

    /// Parse lambda with simple identifier parameter
    fn parse_lambda(&mut self, start: Span) -> Option<Spanned<Expr>> {
        if let TokenKind::Ident(name) = &self.current.kind {
            let name = name.clone();
            self.advance();

            // Check for @ pattern
            if self.consume(&TokenKind::At) {
                // ident @ { ... }: body
                if !self.check(&TokenKind::LBrace) {
                    self.error("Expected { after @");
                    self.synchronize();
                    return None;
                }
                return self.parse_lambda_with_at_pattern(start, name);
            }

            // Simple lambda: ident: body
            if self.consume(&TokenKind::Colon) {
                let body = self.expr()?;
                let lambda = Lambda {
                    param: LambdaParam::Ident(name),
                    body: Box::new(body),
                };
                return Some(Spanned::new(Expr::Lambda(lambda), self.span_from(&start)));
            }

            // Not a lambda, backtrack and parse as expression
            // For now, just parse the identifier as part of an expression
            let ident_span = self.span_from(&start);
            let ident_expr = Spanned::new(Expr::Ident(name), ident_span);
            return self.continue_expr_from(ident_expr);
        }

        self.expr_op()
    }

    /// Parse lambda with @-pattern (name @ { ... })
    fn parse_lambda_with_at_pattern(&mut self, start: Span, at_name: String) -> Option<Spanned<Expr>> {
        self.advance(); // consume {

        let mut entries = Vec::new();
        let mut ellipsis = false;

        loop {
            match &self.current.kind {
                TokenKind::Ellipsis => {
                    ellipsis = true;
                    self.advance();
                }
                TokenKind::Ident(name) => {
                    let name_span = self.current.span.clone();
                    let name = name.clone();
                    self.advance();

                    let default = if self.consume(&TokenKind::Question) {
                        self.expr()
                    } else {
                        None
                    };

                    entries.push(PatternEntry {
                        name: Spanned::new(name, name_span),
                        default,
                    });
                }
                TokenKind::RBrace => break,
                TokenKind::Comma => {
                    self.advance();
                    continue;
                }
                _ => {
                    self.error(format!("Unexpected token in pattern: {:?}", self.current.kind));
                    self.synchronize();
                    return None;
                }
            }

            if !self.consume(&TokenKind::Comma) {
                break;
            }
        }

        if self.expect(TokenKind::RBrace).is_err() {
            self.synchronize();
            return None;
        }

        if self.expect(TokenKind::Colon).is_err() {
            self.synchronize();
            return None;
        }

        let body = self.expr()?;

        let lambda = Lambda {
            param: LambdaParam::Pattern {
                entries,
                ellipsis,
                at: Some(at_name),
            },
            body: Box::new(body),
        };

        Some(Spanned::new(Expr::Lambda(lambda), self.span_from(&start)))
    }

    /// Continue parsing an expression from a parsed prefix
    fn continue_expr_from(&mut self, prefix: Spanned<Expr>) -> Option<Spanned<Expr>> {
        self.expr_postfix_from(prefix)
    }

    /// Parse operation expression (handles precedence)
    fn expr_op(&mut self) -> Option<Spanned<Expr>> {
        self.expr_implication()
    }

    /// Parse implication (lowest precedence, right-associative)
    fn expr_implication(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_or()?;

        while self.consume(&TokenKind::Implication) {
            let right = self.expr_implication()?; // Right-associative
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op: BinOp::Implication,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse logical OR
    fn expr_or(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_and()?;

        while self.consume(&TokenKind::OrOp) {
            let right = self.expr_and()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op: BinOp::Or,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse logical AND
    fn expr_and(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_equality()?;

        while self.consume(&TokenKind::And) {
            let right = self.expr_equality()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op: BinOp::And,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse equality comparison
    fn expr_equality(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_comparison()?;

        loop {
            let op = if self.consume(&TokenKind::EqEq) {
                BinOp::Eq
            } else if self.consume(&TokenKind::NotEq) {
                BinOp::NotEq
            } else {
                break;
            };

            let right = self.expr_comparison()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse comparison operators
    fn expr_comparison(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_update()?;

        loop {
            let op = if self.consume(&TokenKind::Lt) {
                BinOp::Lt
            } else if self.consume(&TokenKind::Gt) {
                BinOp::Gt
            } else if self.consume(&TokenKind::LtEq) {
                BinOp::LtEq
            } else if self.consume(&TokenKind::GtEq) {
                BinOp::GtEq
            } else {
                break;
            };

            let right = self.expr_update()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse update operator (//)
    fn expr_update(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_not()?;

        while self.consume(&TokenKind::Update) {
            let right = self.expr_not()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op: BinOp::Update,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse logical NOT
    fn expr_not(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();

        if self.consume(&TokenKind::Not) {
            let expr = self.expr_not()?;
            return Some(Spanned::new(
                Expr::UnaryOp {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                },
                self.span_from(&start),
            ));
        }

        self.expr_concat()
    }

    /// Parse concatenation (++)
    fn expr_concat(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_additive()?;

        while self.consume(&TokenKind::Concat) {
            let right = self.expr_additive()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op: BinOp::Concat,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse additive operators (+ -)
    fn expr_additive(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_multiplicative()?;

        loop {
            let op = if self.consume(&TokenKind::Plus) {
                BinOp::Add
            } else if self.consume(&TokenKind::Minus) {
                BinOp::Sub
            } else {
                break;
            };

            let right = self.expr_multiplicative()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse multiplicative operators (* /)
    fn expr_multiplicative(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut left = self.expr_negate()?;

        loop {
            let op = if self.consume(&TokenKind::Star) {
                BinOp::Mul
            } else if self.consume(&TokenKind::Slash) {
                BinOp::Div
            } else {
                break;
            };

            let right = self.expr_negate()?;
            left = Spanned::new(
                Expr::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                self.span_from(&start),
            );
        }

        Some(left)
    }

    /// Parse negation
    fn expr_negate(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();

        if self.consume(&TokenKind::Minus) {
            let expr = self.expr_negate()?;
            return Some(Spanned::new(
                Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                },
                self.span_from(&start),
            ));
        }

        self.expr_has_attr()
    }

    /// Parse has-attribute (?)
    fn expr_has_attr(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut expr = self.expr_apply()?;

        while self.consume(&TokenKind::Question) {
            let path = self.parse_attr_path()?;
            expr = Spanned::new(
                Expr::HasAttr {
                    expr: Box::new(expr),
                    path,
                },
                self.span_from(&start),
            );
        }

        Some(expr)
    }

    /// Parse function application
    fn expr_apply(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut expr = self.expr_select()?;

        // Function application is left-associative juxtaposition
        loop {
            // Check if next token can start an argument
            if !self.can_start_expr() {
                break;
            }

            // Don't consume operators or delimiters
            match &self.current.kind {
                TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Concat
                | TokenKind::Update
                | TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::LtEq
                | TokenKind::GtEq
                | TokenKind::And
                | TokenKind::OrOp
                | TokenKind::Implication
                | TokenKind::Question
                | TokenKind::Colon
                | TokenKind::Semicolon
                | TokenKind::Comma
                | TokenKind::RBrace
                | TokenKind::RBracket
                | TokenKind::RParen
                | TokenKind::Then
                | TokenKind::Else
                | TokenKind::In => break,
                _ => {}
            }

            let arg = self.expr_select()?;
            expr = Spanned::new(
                Expr::Apply {
                    func: Box::new(expr),
                    arg: Box::new(arg),
                },
                self.span_from(&start),
            );
        }

        Some(expr)
    }

    /// Check if current token can start an expression
    fn can_start_expr(&self) -> bool {
        matches!(
            self.current.kind,
            TokenKind::Ident(_)
                | TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::String(_)
                | TokenKind::IndentedString(_)
                | TokenKind::Path(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
                | TokenKind::LBrace
                | TokenKind::LBracket
                | TokenKind::LParen
                | TokenKind::Let
                | TokenKind::If
                | TokenKind::With
                | TokenKind::Assert
                | TokenKind::Rec
        )
    }

    /// Parse select expression (attribute access)
    fn expr_select(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();
        let mut expr = self.expr_primary()?;

        while self.consume(&TokenKind::Dot) {
            let path = self.parse_attr_path()?;

            // Check for `or default`
            let default = if self.consume(&TokenKind::Or) {
                Some(Box::new(self.expr_select()?))
            } else {
                None
            };

            expr = Spanned::new(
                Expr::Select {
                    expr: Box::new(expr),
                    path,
                    default,
                },
                self.span_from(&start),
            );
        }

        Some(expr)
    }

    /// Continue parsing postfix operations from a prefix
    fn expr_postfix_from(&mut self, mut expr: Spanned<Expr>) -> Option<Spanned<Expr>> {
        let start = expr.span.clone();

        // Handle select
        while self.consume(&TokenKind::Dot) {
            let path = self.parse_attr_path()?;
            let default = if self.consume(&TokenKind::Or) {
                Some(Box::new(self.expr_select()?))
            } else {
                None
            };

            expr = Spanned::new(
                Expr::Select {
                    expr: Box::new(expr),
                    path,
                    default,
                },
                self.span_from(&start),
            );
        }

        // Handle function application
        while self.can_start_expr() {
            match &self.current.kind {
                TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Concat
                | TokenKind::Update
                | TokenKind::EqEq
                | TokenKind::NotEq
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::LtEq
                | TokenKind::GtEq
                | TokenKind::And
                | TokenKind::OrOp
                | TokenKind::Implication
                | TokenKind::Question
                | TokenKind::Colon
                | TokenKind::Semicolon
                | TokenKind::Comma
                | TokenKind::RBrace
                | TokenKind::RBracket
                | TokenKind::RParen
                | TokenKind::Then
                | TokenKind::Else
                | TokenKind::In => break,
                _ => {}
            }

            let arg = self.expr_select()?;
            expr = Spanned::new(
                Expr::Apply {
                    func: Box::new(expr),
                    arg: Box::new(arg),
                },
                self.span_from(&start),
            );
        }

        // Continue with operators
        self.continue_binop_from(expr)
    }

    /// Continue parsing binary operations from a left-hand side
    fn continue_binop_from(&mut self, left: Spanned<Expr>) -> Option<Spanned<Expr>> {
        // This is a simplified version - ideally we'd refactor the precedence parsing
        // For now, just handle common cases
        let start = left.span.clone();
        let mut result = left;

        // Handle operators in precedence order
        loop {
            if self.consume(&TokenKind::Update) {
                let right = self.expr_not()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::Update,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else if self.consume(&TokenKind::Plus) {
                let right = self.expr_multiplicative()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::Add,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else if self.consume(&TokenKind::Minus) {
                let right = self.expr_multiplicative()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::Sub,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else if self.consume(&TokenKind::EqEq) {
                let right = self.expr_comparison()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::Eq,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else if self.consume(&TokenKind::And) {
                let right = self.expr_equality()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::And,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else if self.consume(&TokenKind::OrOp) {
                let right = self.expr_and()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::Or,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else if self.consume(&TokenKind::Implication) {
                let right = self.expr_implication()?;
                result = Spanned::new(
                    Expr::BinOp {
                        left: Box::new(result),
                        op: BinOp::Implication,
                        right: Box::new(right),
                    },
                    self.span_from(&start),
                );
            } else {
                break;
            }
        }

        Some(result)
    }

    /// Parse attribute path
    fn parse_attr_path(&mut self) -> Option<Vec<Spanned<AttrName>>> {
        let mut path = Vec::new();

        loop {
            let start = self.current.span.clone();
            let name = self.parse_attr_name()?;
            path.push(Spanned::new(name, self.span_from(&start)));

            if !self.consume(&TokenKind::Dot) {
                break;
            }
        }

        Some(path)
    }

    /// Parse a single attribute name
    fn parse_attr_name(&mut self) -> Option<AttrName> {
        match &self.current.kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Some(AttrName::Ident(name))
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Some(AttrName::String(StringParts::simple(s)))
            }
            TokenKind::Interpolation => {
                self.advance();
                let expr = self.expr()?;
                if self.expect(TokenKind::RBrace).is_err() {
                    return None;
                }
                Some(AttrName::Interpolation(Box::new(expr)))
            }
            // Keywords can be used as attribute names
            TokenKind::If
            | TokenKind::Then
            | TokenKind::Else
            | TokenKind::Let
            | TokenKind::In
            | TokenKind::Rec
            | TokenKind::With
            | TokenKind::Inherit
            | TokenKind::Assert
            | TokenKind::Or => {
                let name = format!("{:?}", self.current.kind).to_lowercase();
                self.advance();
                Some(AttrName::Ident(name))
            }
            _ => {
                self.error(format!("Expected attribute name, found {:?}", self.current.kind));
                None
            }
        }
    }

    /// Parse primary expression
    fn expr_primary(&mut self) -> Option<Spanned<Expr>> {
        let start = self.current.span.clone();

        match &self.current.kind {
            // Literals
            TokenKind::Null => {
                self.advance();
                Some(Spanned::new(Expr::Null, self.span_from(&start)))
            }
            TokenKind::True => {
                self.advance();
                Some(Spanned::new(Expr::Bool(true), self.span_from(&start)))
            }
            TokenKind::False => {
                self.advance();
                Some(Spanned::new(Expr::Bool(false), self.span_from(&start)))
            }
            TokenKind::Int(n) => {
                let n = *n;
                self.advance();
                Some(Spanned::new(Expr::Int(n), self.span_from(&start)))
            }
            TokenKind::Float(f) => {
                let f = *f;
                self.advance();
                Some(Spanned::new(Expr::Float(f), self.span_from(&start)))
            }
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Some(Spanned::new(
                    Expr::String(StringParts::simple(s)),
                    self.span_from(&start),
                ))
            }
            TokenKind::IndentedString(s) => {
                let s = s.clone();
                self.advance();
                Some(Spanned::new(
                    Expr::String(StringParts::simple(s)),
                    self.span_from(&start),
                ))
            }
            TokenKind::Path(p) => {
                let p = PathBuf::from(p.clone());
                self.advance();
                Some(Spanned::new(Expr::Path(p), self.span_from(&start)))
            }

            // Identifier
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Some(Spanned::new(Expr::Ident(name), self.span_from(&start)))
            }

            // Parenthesized expression
            TokenKind::LParen => {
                self.advance();
                let expr = self.expr()?;
                if self.expect(TokenKind::RParen).is_err() {
                    self.synchronize();
                    return Some(Spanned::new(Expr::Error, self.span_from(&start)));
                }
                Some(expr)
            }

            // List
            TokenKind::LBracket => self.parse_list(start),

            // Attribute set or recursive attribute set
            TokenKind::LBrace => self.parse_attrset(start, false),
            TokenKind::Rec => {
                self.advance();
                if !self.check(&TokenKind::LBrace) {
                    self.error("Expected { after rec");
                    return None;
                }
                let brace_start = self.current.span.clone();
                self.parse_attrset(brace_start, true)
            }

            // Let expression
            TokenKind::Let => self.parse_let(start),

            // If expression
            TokenKind::If => self.parse_if(start),

            // With expression
            TokenKind::With => self.parse_with(start),

            // Assert expression
            TokenKind::Assert => self.parse_assert(start),

            _ => {
                self.error(format!("Unexpected token: {:?}", self.current.kind));
                None
            }
        }
    }

    /// Parse list expression
    fn parse_list(&mut self, start: Span) -> Option<Spanned<Expr>> {
        self.advance(); // consume [

        let mut elements = Vec::new();

        while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
            if let Some(elem) = self.expr_select() {
                elements.push(elem);
            } else {
                self.synchronize();
                break;
            }
        }

        if self.expect(TokenKind::RBracket).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        Some(Spanned::new(Expr::List(elements), self.span_from(&start)))
    }

    /// Parse attribute set
    fn parse_attrset(&mut self, start: Span, recursive: bool) -> Option<Spanned<Expr>> {
        self.advance(); // consume {

        let mut bindings = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.consume(&TokenKind::Inherit) {
                // Inherit binding
                let from = if self.consume(&TokenKind::LParen) {
                    let expr = self.expr()?;
                    if self.expect(TokenKind::RParen).is_err() {
                        self.synchronize();
                        continue;
                    }
                    Some(expr)
                } else {
                    None
                };

                let mut names = Vec::new();
                while let TokenKind::Ident(name) = &self.current.kind {
                    let name_span = self.current.span.clone();
                    names.push(Spanned::new(name.clone(), name_span));
                    self.advance();
                }

                if self.expect(TokenKind::Semicolon).is_err() {
                    self.synchronize();
                    continue;
                }

                bindings.push(Binding::Inherit { from, names });
            } else {
                // Simple binding: path = value;
                let path = match self.parse_attr_path() {
                    Some(p) => p,
                    None => {
                        self.synchronize();
                        continue;
                    }
                };

                if self.expect(TokenKind::Eq).is_err() {
                    self.synchronize();
                    continue;
                }

                let value = match self.expr() {
                    Some(v) => v,
                    None => {
                        self.synchronize();
                        continue;
                    }
                };

                if self.expect(TokenKind::Semicolon).is_err() {
                    self.synchronize();
                    continue;
                }

                bindings.push(Binding::Simple { path, value });
            }
        }

        if self.expect(TokenKind::RBrace).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        Some(Spanned::new(
            Expr::AttrSet(AttrSet { recursive, bindings }),
            self.span_from(&start),
        ))
    }

    /// Parse let expression
    fn parse_let(&mut self, start: Span) -> Option<Spanned<Expr>> {
        self.advance(); // consume let

        let mut bindings = Vec::new();

        while !self.check(&TokenKind::In) && !self.check(&TokenKind::Eof) {
            if self.consume(&TokenKind::Inherit) {
                // Inherit binding
                let from = if self.consume(&TokenKind::LParen) {
                    let expr = self.expr()?;
                    if self.expect(TokenKind::RParen).is_err() {
                        self.synchronize();
                        continue;
                    }
                    Some(expr)
                } else {
                    None
                };

                let mut names = Vec::new();
                while let TokenKind::Ident(name) = &self.current.kind {
                    let name_span = self.current.span.clone();
                    names.push(Spanned::new(name.clone(), name_span));
                    self.advance();
                }

                if self.expect(TokenKind::Semicolon).is_err() {
                    self.synchronize();
                    continue;
                }

                bindings.push(Binding::Inherit { from, names });
            } else {
                let path = match self.parse_attr_path() {
                    Some(p) => p,
                    None => {
                        self.synchronize();
                        continue;
                    }
                };

                if self.expect(TokenKind::Eq).is_err() {
                    self.synchronize();
                    continue;
                }

                let value = match self.expr() {
                    Some(v) => v,
                    None => {
                        self.synchronize();
                        continue;
                    }
                };

                if self.expect(TokenKind::Semicolon).is_err() {
                    self.synchronize();
                    continue;
                }

                bindings.push(Binding::Simple { path, value });
            }
        }

        if self.expect(TokenKind::In).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        let body = self.expr()?;

        Some(Spanned::new(
            Expr::Let {
                bindings,
                body: Box::new(body),
            },
            self.span_from(&start),
        ))
    }

    /// Parse if expression
    fn parse_if(&mut self, start: Span) -> Option<Spanned<Expr>> {
        self.advance(); // consume if

        let cond = self.expr()?;

        if self.expect(TokenKind::Then).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        let then_expr = self.expr()?;

        if self.expect(TokenKind::Else).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        let else_expr = self.expr()?;

        Some(Spanned::new(
            Expr::If {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
            self.span_from(&start),
        ))
    }

    /// Parse with expression
    fn parse_with(&mut self, start: Span) -> Option<Spanned<Expr>> {
        self.advance(); // consume with

        let env = self.expr()?;

        if self.expect(TokenKind::Semicolon).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        let body = self.expr()?;

        Some(Spanned::new(
            Expr::With {
                env: Box::new(env),
                body: Box::new(body),
            },
            self.span_from(&start),
        ))
    }

    /// Parse assert expression
    fn parse_assert(&mut self, start: Span) -> Option<Spanned<Expr>> {
        self.advance(); // consume assert

        let cond = self.expr()?;

        if self.expect(TokenKind::Semicolon).is_err() {
            self.synchronize();
            return Some(Spanned::new(Expr::Error, self.span_from(&start)));
        }

        let body = self.expr()?;

        Some(Spanned::new(
            Expr::Assert {
                cond: Box::new(cond),
                body: Box::new(body),
            },
            self.span_from(&start),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_expr(source: &str) -> Result<Spanned<Expr>, Vec<ParseError>> {
        let mut parser = Parser::new(source, PathBuf::from("test.nix"));
        parser.parse_expr()
    }

    #[test]
    fn test_parse_null() {
        let result = parse_expr("null");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::Null));
    }

    #[test]
    fn test_parse_bool() {
        let result = parse_expr("true");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::Bool(true)));

        let result = parse_expr("false");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::Bool(false)));
    }

    #[test]
    fn test_parse_int() {
        let result = parse_expr("42");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::Int(42)));
    }

    #[test]
    fn test_parse_string() {
        let result = parse_expr(r#""hello""#);
        assert!(result.is_ok());
        if let Expr::String(parts) = result.unwrap().node {
            assert_eq!(parts.as_simple(), Some("hello"));
        } else {
            panic!("Expected String");
        }
    }

    #[test]
    fn test_parse_list() {
        let result = parse_expr("[1 2 3]");
        assert!(result.is_ok());
        if let Expr::List(elements) = result.unwrap().node {
            assert_eq!(elements.len(), 3);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_parse_attrset() {
        let result = parse_expr("{ x = 1; y = 2; }");
        assert!(result.is_ok());
        if let Expr::AttrSet(attrs) = result.unwrap().node {
            assert_eq!(attrs.bindings.len(), 2);
            assert!(!attrs.recursive);
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_rec_attrset() {
        let result = parse_expr("rec { x = 1; y = x; }");
        assert!(result.is_ok());
        if let Expr::AttrSet(attrs) = result.unwrap().node {
            assert!(attrs.recursive);
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_simple_lambda() {
        let result = parse_expr("x: x + 1");
        assert!(result.is_ok());
        if let Expr::Lambda(lambda) = result.unwrap().node {
            assert!(matches!(lambda.param, LambdaParam::Ident(ref s) if s == "x"));
        } else {
            panic!("Expected Lambda");
        }
    }

    #[test]
    fn test_parse_pattern_lambda() {
        let result = parse_expr("{ x, y }: x + y");
        assert!(result.is_ok());
        if let Expr::Lambda(lambda) = result.unwrap().node {
            if let LambdaParam::Pattern { entries, ellipsis, .. } = lambda.param {
                assert_eq!(entries.len(), 2);
                assert!(!ellipsis);
            } else {
                panic!("Expected Pattern");
            }
        } else {
            panic!("Expected Lambda");
        }
    }

    #[test]
    fn test_parse_pattern_with_ellipsis() {
        let result = parse_expr("{ x, ... }: x");
        assert!(result.is_ok());
        if let Expr::Lambda(lambda) = result.unwrap().node {
            if let LambdaParam::Pattern { ellipsis, .. } = lambda.param {
                assert!(ellipsis);
            } else {
                panic!("Expected Pattern");
            }
        } else {
            panic!("Expected Lambda");
        }
    }

    #[test]
    fn test_parse_pattern_with_defaults() {
        let result = parse_expr("{ x ? 1, y ? 2 }: x + y");
        assert!(result.is_ok());
        if let Expr::Lambda(lambda) = result.unwrap().node {
            if let LambdaParam::Pattern { entries, .. } = lambda.param {
                assert!(entries[0].default.is_some());
                assert!(entries[1].default.is_some());
            } else {
                panic!("Expected Pattern");
            }
        } else {
            panic!("Expected Lambda");
        }
    }

    #[test]
    fn test_parse_let() {
        let result = parse_expr("let x = 1; in x");
        assert!(result.is_ok());
        if let Expr::Let { bindings, body: _ } = result.unwrap().node {
            assert_eq!(bindings.len(), 1);
        } else {
            panic!("Expected Let");
        }
    }

    #[test]
    fn test_parse_if() {
        let result = parse_expr("if true then 1 else 2");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::If { .. }));
    }

    #[test]
    fn test_parse_with() {
        let result = parse_expr("with { x = 1; }; x");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::With { .. }));
    }

    #[test]
    fn test_parse_binop() {
        let result = parse_expr("1 + 2 * 3");
        assert!(result.is_ok());
        // Should be 1 + (2 * 3) due to precedence
        if let Expr::BinOp { op, .. } = result.unwrap().node {
            assert_eq!(op, BinOp::Add);
        } else {
            panic!("Expected BinOp");
        }
    }

    #[test]
    fn test_parse_function_application() {
        let result = parse_expr("f x y");
        assert!(result.is_ok());
        // Should be (f x) y
        if let Expr::Apply { func, arg: _ } = result.unwrap().node {
            assert!(matches!(func.node, Expr::Apply { .. }));
        } else {
            panic!("Expected Apply");
        }
    }

    #[test]
    fn test_parse_select() {
        let result = parse_expr("x.y.z");
        assert!(result.is_ok());
        if let Expr::Select { path, .. } = result.unwrap().node {
            assert_eq!(path.len(), 2); // y and z
        } else {
            panic!("Expected Select");
        }
    }

    #[test]
    fn test_parse_select_with_default() {
        let result = parse_expr("x.y or 1");
        assert!(result.is_ok());
        if let Expr::Select { default, .. } = result.unwrap().node {
            assert!(default.is_some());
        } else {
            panic!("Expected Select");
        }
    }

    #[test]
    fn test_parse_module() {
        let result = parse_expr("{ config, lib, ... }: { options = {}; config = {}; }");
        assert!(result.is_ok());
        if let Expr::Lambda(lambda) = result.unwrap().node {
            if let LambdaParam::Pattern { entries, ellipsis, .. } = lambda.param {
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
    fn test_parse_mkoption_call() {
        let result = parse_expr(r#"mkOption { type = types.bool; default = false; }"#);
        assert!(result.is_ok());
        if let Expr::Apply { func, arg } = result.unwrap().node {
            assert!(matches!(func.node, Expr::Ident(ref s) if s == "mkOption"));
            assert!(matches!(arg.node, Expr::AttrSet(_)));
        } else {
            panic!("Expected Apply");
        }
    }

    #[test]
    fn test_parse_mkif_call() {
        let result = parse_expr("mkIf config.enable { setting = true; }");
        assert!(result.is_ok());
        // mkIf applied to config.enable, then to the attrset
        if let Expr::Apply { func, .. } = result.unwrap().node {
            if let Expr::Apply { func: inner_func, .. } = func.node {
                assert!(matches!(inner_func.node, Expr::Ident(ref s) if s == "mkIf"));
            } else {
                panic!("Expected nested Apply");
            }
        } else {
            panic!("Expected Apply");
        }
    }

    #[test]
    fn test_parse_inherit() {
        let result = parse_expr("{ inherit x y; z = 1; }");
        assert!(result.is_ok());
        if let Expr::AttrSet(attrs) = result.unwrap().node {
            assert_eq!(attrs.bindings.len(), 2);
            assert!(matches!(&attrs.bindings[0], Binding::Inherit { names, .. } if names.len() == 2));
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_inherit_from() {
        let result = parse_expr("{ inherit (lib) mkOption mkIf; }");
        assert!(result.is_ok());
        if let Expr::AttrSet(attrs) = result.unwrap().node {
            if let Binding::Inherit { from, names } = &attrs.bindings[0] {
                assert!(from.is_some());
                assert_eq!(names.len(), 2);
            } else {
                panic!("Expected Inherit binding");
            }
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_update_operator() {
        let result = parse_expr("{ a = 1; } // { b = 2; }");
        assert!(result.is_ok());
        if let Expr::BinOp { op, .. } = result.unwrap().node {
            assert_eq!(op, BinOp::Update);
        } else {
            panic!("Expected BinOp");
        }
    }

    #[test]
    fn test_parse_nested_attrset() {
        let result = parse_expr("{ a.b.c = 1; }");
        assert!(result.is_ok());
        if let Expr::AttrSet(attrs) = result.unwrap().node {
            if let Binding::Simple { path, .. } = &attrs.bindings[0] {
                assert_eq!(path.len(), 3);
            } else {
                panic!("Expected Simple binding");
            }
        } else {
            panic!("Expected AttrSet");
        }
    }

    #[test]
    fn test_parse_assert() {
        let result = parse_expr("assert x > 0; x");
        assert!(result.is_ok());
        assert!(matches!(result.unwrap().node, Expr::Assert { .. }));
    }
}
