//! Lexer for Nix expressions.
//!
//! This module provides tokenization of Nix source code.

use crate::types::Span;
use std::path::PathBuf;

/// Token types for Nix lexer
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    /// Identifier (e.g., `config`, `lib`, `mkOption`)
    Ident(String),
    /// String literal (e.g., `"hello"`)
    String(String),
    /// Multi-line string (e.g., `''hello''`)
    IndentedString(String),
    /// Integer literal (e.g., `42`)
    Int(i64),
    /// Float literal (e.g., `3.14`)
    Float(f64),
    /// Path literal (e.g., `./foo/bar`)
    Path(String),
    /// URI literal (e.g., `https://example.com`)
    Uri(String),

    // Keywords
    /// `if`
    If,
    /// `then`
    Then,
    /// `else`
    Else,
    /// `let`
    Let,
    /// `in`
    In,
    /// `rec`
    Rec,
    /// `with`
    With,
    /// `inherit`
    Inherit,
    /// `assert`
    Assert,
    /// `or` (keyword, not operator)
    Or,
    /// `null`
    Null,
    /// `true`
    True,
    /// `false`
    False,

    // Delimiters
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `(`
    LParen,
    /// `)`
    RParen,

    // Operators
    /// `=`
    Eq,
    /// `==`
    EqEq,
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
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `++`
    Concat,
    /// `//`
    Update,
    /// `&&`
    And,
    /// `||`
    OrOp,
    /// `!`
    Not,
    /// `->`
    Implication,
    /// `?`
    Question,
    /// `@`
    At,

    // Punctuation
    /// `.`
    Dot,
    /// `;`
    Semicolon,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `...`
    Ellipsis,

    // Special
    /// `${` - String interpolation start
    Interpolation,

    // Meta
    /// End of file
    Eof,
    /// Invalid token (for error recovery)
    Error(String),
}

/// A token with span information
#[derive(Debug, Clone)]
pub struct Token {
    /// The token kind
    pub kind: TokenKind,
    /// Source location
    pub span: Span,
}

impl Token {
    /// Create a new token
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Lexer for Nix source code
pub struct Lexer {
    /// Source code
    source: Vec<char>,
    /// Current position in source
    pos: usize,
    /// Current line number (1-indexed)
    line: usize,
    /// Current column number (1-indexed)
    column: usize,
    /// Start position of current token
    token_start: usize,
    /// Start line of current token
    token_start_line: usize,
    /// Start column of current token
    token_start_column: usize,
    /// Source file path
    file: PathBuf,
}

impl Lexer {
    /// Create a new lexer for the given source
    pub fn new(source: &str, file: PathBuf) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            token_start: 0,
            token_start_line: 1,
            token_start_column: 1,
            file,
        }
    }

    /// Peek at the current character without consuming it
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    /// Peek at the next character without consuming
    fn peek_next(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    /// Advance to the next character
    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    /// Mark the start of a new token
    fn start_token(&mut self) {
        self.token_start = self.pos;
        self.token_start_line = self.line;
        self.token_start_column = self.column;
    }

    /// Create a span for the current token
    fn make_span(&self) -> Span {
        Span::new(
            self.file.clone(),
            self.token_start,
            self.pos,
            self.token_start_line,
            self.token_start_column,
        )
    }

    /// Create a token with the current span
    fn make_token(&self, kind: TokenKind) -> Token {
        Token::new(kind, self.make_span())
    }

    /// Skip whitespace and comments
    fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\n') | Some('\r') => {
                    self.advance();
                }
                Some('#') => {
                    // Line comment
                    while let Some(ch) = self.peek() {
                        if ch == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                Some('/') if self.peek_next() == Some('*') => {
                    // Block comment
                    self.advance(); // /
                    self.advance(); // *
                    let mut depth = 1;
                    while depth > 0 {
                        match (self.peek(), self.peek_next()) {
                            (Some('*'), Some('/')) => {
                                self.advance();
                                self.advance();
                                depth -= 1;
                            }
                            (Some('/'), Some('*')) => {
                                self.advance();
                                self.advance();
                                depth += 1;
                            }
                            (Some(_), _) => {
                                self.advance();
                            }
                            (None, _) => break,
                        }
                    }
                }
                _ => break,
            }
        }
    }

    /// Check if character can start an identifier
    fn is_ident_start(ch: char) -> bool {
        ch.is_ascii_alphabetic() || ch == '_'
    }

    /// Check if character can continue an identifier
    fn is_ident_continue(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '\''
    }

    /// Scan an identifier or keyword
    fn scan_ident(&mut self) -> Token {
        let mut ident = String::new();
        while let Some(ch) = self.peek() {
            if Self::is_ident_continue(ch) {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let kind = match ident.as_str() {
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "let" => TokenKind::Let,
            "in" => TokenKind::In,
            "rec" => TokenKind::Rec,
            "with" => TokenKind::With,
            "inherit" => TokenKind::Inherit,
            "assert" => TokenKind::Assert,
            "or" => TokenKind::Or,
            "null" => TokenKind::Null,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(ident),
        };

        self.make_token(kind)
    }

    /// Scan a number (integer or float)
    fn scan_number(&mut self) -> Token {
        let mut num = String::new();
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                // Check if next char is a digit (not a path like ./foo)
                if let Some(next) = self.peek_next() {
                    if next.is_ascii_digit() {
                        is_float = true;
                        num.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Handle scientific notation
        if let Some('e') | Some('E') = self.peek() {
            is_float = true;
            num.push(self.advance().unwrap());
            if let Some('+') | Some('-') = self.peek() {
                num.push(self.advance().unwrap());
            }
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    num.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let kind = if is_float {
            match num.parse::<f64>() {
                Ok(f) => TokenKind::Float(f),
                Err(e) => TokenKind::Error(format!("Invalid float: {}", e)),
            }
        } else {
            match num.parse::<i64>() {
                Ok(i) => TokenKind::Int(i),
                Err(e) => TokenKind::Error(format!("Invalid integer: {}", e)),
            }
        };

        self.make_token(kind)
    }

    /// Scan a string literal
    fn scan_string(&mut self) -> Token {
        // Opening quote already consumed
        let mut string = String::new();

        loop {
            match self.peek() {
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => {
                            string.push('\n');
                            self.advance();
                        }
                        Some('r') => {
                            string.push('\r');
                            self.advance();
                        }
                        Some('t') => {
                            string.push('\t');
                            self.advance();
                        }
                        Some('\\') => {
                            string.push('\\');
                            self.advance();
                        }
                        Some('"') => {
                            string.push('"');
                            self.advance();
                        }
                        Some('$') => {
                            string.push('$');
                            self.advance();
                        }
                        Some(ch) => {
                            string.push('\\');
                            string.push(ch);
                            self.advance();
                        }
                        None => break,
                    }
                }
                Some('$') if self.peek_next() == Some('{') => {
                    // For now, we don't handle interpolation in strings
                    // This would require a more complex tokenizer state machine
                    string.push('$');
                    self.advance();
                }
                Some(ch) => {
                    string.push(ch);
                    self.advance();
                }
                None => {
                    return self.make_token(TokenKind::Error("Unterminated string".into()));
                }
            }
        }

        self.make_token(TokenKind::String(string))
    }

    /// Scan an indented string ('' ... '')
    fn scan_indented_string(&mut self) -> Token {
        // Opening '' already consumed
        let mut string = String::new();

        loop {
            match self.peek() {
                Some('\'') if self.peek_next() == Some('\'') => {
                    // Check for escape or end
                    self.advance(); // first '
                    match self.peek_next() {
                        Some('\'') => {
                            // ''' = escaped single quote
                            self.advance(); // second '
                            string.push('\'');
                        }
                        Some('$') => {
                            // ''$ = escaped $
                            self.advance(); // second '
                            self.advance(); // $
                            string.push('$');
                        }
                        Some('\\') => {
                            // ''\ = escape sequence
                            self.advance(); // second '
                            self.advance(); // backslash
                            if let Some(ch) = self.peek() {
                                match ch {
                                    'n' => string.push('\n'),
                                    'r' => string.push('\r'),
                                    't' => string.push('\t'),
                                    _ => {
                                        string.push('\\');
                                        string.push(ch);
                                    }
                                }
                                self.advance();
                            }
                        }
                        _ => {
                            // End of indented string
                            self.advance(); // second '
                            break;
                        }
                    }
                }
                Some(ch) => {
                    string.push(ch);
                    self.advance();
                }
                None => {
                    return self.make_token(TokenKind::Error("Unterminated indented string".into()));
                }
            }
        }

        self.make_token(TokenKind::IndentedString(string))
    }

    /// Scan a path literal
    fn scan_path(&mut self, first: char) -> Token {
        let mut path = String::new();
        path.push(first);

        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric()
                || ch == '/'
                || ch == '.'
                || ch == '_'
                || ch == '-'
                || ch == '+'
            {
                path.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        self.make_token(TokenKind::Path(path))
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        self.start_token();

        let ch = match self.advance() {
            Some(ch) => ch,
            None => return self.make_token(TokenKind::Eof),
        };

        match ch {
            // Single character tokens
            '{' => self.make_token(TokenKind::LBrace),
            '}' => self.make_token(TokenKind::RBrace),
            '[' => self.make_token(TokenKind::LBracket),
            ']' => self.make_token(TokenKind::RBracket),
            '(' => self.make_token(TokenKind::LParen),
            ')' => self.make_token(TokenKind::RParen),
            ';' => self.make_token(TokenKind::Semicolon),
            ',' => self.make_token(TokenKind::Comma),
            '@' => self.make_token(TokenKind::At),
            '?' => self.make_token(TokenKind::Question),

            // Potentially multi-character tokens
            ':' => self.make_token(TokenKind::Colon),

            '.' => {
                if self.peek() == Some('.') && self.peek_next() == Some('.') {
                    self.advance();
                    self.advance();
                    self.make_token(TokenKind::Ellipsis)
                } else if self.peek() == Some('/') {
                    // Path starting with ./
                    self.scan_path('.')
                } else {
                    self.make_token(TokenKind::Dot)
                }
            }

            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::EqEq)
                } else {
                    self.make_token(TokenKind::Eq)
                }
            }

            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::NotEq)
                } else {
                    self.make_token(TokenKind::Not)
                }
            }

            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::LtEq)
                } else {
                    self.make_token(TokenKind::Lt)
                }
            }

            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::GtEq)
                } else {
                    self.make_token(TokenKind::Gt)
                }
            }

            '+' => {
                if self.peek() == Some('+') {
                    self.advance();
                    self.make_token(TokenKind::Concat)
                } else {
                    self.make_token(TokenKind::Plus)
                }
            }

            '-' => {
                if self.peek() == Some('>') {
                    self.advance();
                    self.make_token(TokenKind::Implication)
                } else {
                    self.make_token(TokenKind::Minus)
                }
            }

            '*' => self.make_token(TokenKind::Star),

            '/' => {
                if self.peek() == Some('/') {
                    self.advance();
                    self.make_token(TokenKind::Update)
                } else {
                    self.make_token(TokenKind::Slash)
                }
            }

            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    self.make_token(TokenKind::And)
                } else {
                    self.make_token(TokenKind::Error(format!("Unexpected character: {}", ch)))
                }
            }

            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    self.make_token(TokenKind::OrOp)
                } else {
                    self.make_token(TokenKind::Error(format!("Unexpected character: {}", ch)))
                }
            }

            '$' => {
                if self.peek() == Some('{') {
                    self.advance();
                    self.make_token(TokenKind::Interpolation)
                } else {
                    self.make_token(TokenKind::Error(format!("Unexpected character: {}", ch)))
                }
            }

            '"' => self.scan_string(),

            '\'' => {
                if self.peek() == Some('\'') {
                    self.advance();
                    self.scan_indented_string()
                } else {
                    // Single quote is part of identifier (like foo')
                    self.make_token(TokenKind::Error("Unexpected single quote".into()))
                }
            }

            '~' => {
                // Home path ~/...
                if self.peek() == Some('/') {
                    self.scan_path('~')
                } else {
                    self.make_token(TokenKind::Error(format!("Unexpected character: {}", ch)))
                }
            }

            _ if Self::is_ident_start(ch) => {
                // Put the char back and rescan
                self.pos -= 1;
                if ch == '\n' {
                    self.line -= 1;
                } else {
                    self.column -= 1;
                }
                self.scan_ident()
            }

            _ if ch.is_ascii_digit() => {
                // Put the char back and rescan
                self.pos -= 1;
                self.column -= 1;
                self.scan_number()
            }

            _ => self.make_token(TokenKind::Error(format!("Unexpected character: {}", ch))),
        }
    }

    /// Tokenize the entire source
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = matches!(token.kind, TokenKind::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    /// Get the remaining source from a given byte offset
    pub fn source_from(&self, offset: usize) -> String {
        self.source[offset..].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source, PathBuf::from("test.nix"));
        lexer
            .tokenize()
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| !matches!(k, TokenKind::Eof))
            .collect()
    }

    #[test]
    fn test_simple_tokens() {
        assert_eq!(
            lex("{ } [ ] ( ) ; : . , @"),
            vec![
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Semicolon,
                TokenKind::Colon,
                TokenKind::Dot,
                TokenKind::Comma,
                TokenKind::At,
            ]
        );
    }

    #[test]
    fn test_operators() {
        assert_eq!(
            lex("= == != < > <= >= + - * / ++ // && || ! -> ?"),
            vec![
                TokenKind::Eq,
                TokenKind::EqEq,
                TokenKind::NotEq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::LtEq,
                TokenKind::GtEq,
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Concat,
                TokenKind::Update,
                TokenKind::And,
                TokenKind::OrOp,
                TokenKind::Not,
                TokenKind::Implication,
                TokenKind::Question,
            ]
        );
    }

    #[test]
    fn test_keywords() {
        assert_eq!(
            lex("if then else let in rec with inherit assert or null true false"),
            vec![
                TokenKind::If,
                TokenKind::Then,
                TokenKind::Else,
                TokenKind::Let,
                TokenKind::In,
                TokenKind::Rec,
                TokenKind::With,
                TokenKind::Inherit,
                TokenKind::Assert,
                TokenKind::Or,
                TokenKind::Null,
                TokenKind::True,
                TokenKind::False,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        assert_eq!(
            lex("foo bar_baz config-option lib'"),
            vec![
                TokenKind::Ident("foo".into()),
                TokenKind::Ident("bar_baz".into()),
                TokenKind::Ident("config-option".into()),
                TokenKind::Ident("lib'".into()),
            ]
        );
    }

    #[test]
    fn test_numbers() {
        assert_eq!(
            lex("42 3.14 1e10 2.5e-3"),
            vec![
                TokenKind::Int(42),
                TokenKind::Float(3.14),
                TokenKind::Float(1e10),
                TokenKind::Float(2.5e-3),
            ]
        );
    }

    #[test]
    fn test_strings() {
        assert_eq!(
            lex(r#""hello" "world\n""#),
            vec![
                TokenKind::String("hello".into()),
                TokenKind::String("world\n".into()),
            ]
        );
    }

    #[test]
    fn test_comments() {
        assert_eq!(
            lex("foo # comment\nbar /* block */ baz"),
            vec![
                TokenKind::Ident("foo".into()),
                TokenKind::Ident("bar".into()),
                TokenKind::Ident("baz".into()),
            ]
        );
    }

    #[test]
    fn test_ellipsis() {
        assert_eq!(lex("{ ... }"), vec![TokenKind::LBrace, TokenKind::Ellipsis, TokenKind::RBrace]);
    }

    #[test]
    fn test_module_pattern() {
        let tokens = lex("{ config, lib, ... }: { }");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBrace,
                TokenKind::Ident("config".into()),
                TokenKind::Comma,
                TokenKind::Ident("lib".into()),
                TokenKind::Comma,
                TokenKind::Ellipsis,
                TokenKind::RBrace,
                TokenKind::Colon,
                TokenKind::LBrace,
                TokenKind::RBrace,
            ]
        );
    }
}
