use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Number(i64),
    Float(f64),
    String(String),
    /// MudOS functional argument placeholder (`$1`, `$2`, …).
    DollarArg(usize),
    Symbol(String),
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

pub fn lex(source: &str) -> Result<Vec<Token>> {
    Lexer::new(source).tokenize()
}

struct Lexer<'a> {
    chars: Vec<char>,
    offset: usize,
    line: usize,
    column: usize,
    _source: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            offset: 0,
            line: 1,
            column: 1,
            _source: source,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_trivia()?;
            let line = self.line;
            let column = self.column;
            let Some(ch) = self.peek(0) else {
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    line,
                    column,
                });
                return Ok(tokens);
            };
            let kind = if ch.is_ascii_alphabetic() || ch == '_' {
                self.identifier()
            } else if ch.is_ascii_digit() {
                self.number()?
            } else if ch == '"' {
                self.string()?
            } else if ch == '\'' {
                self.char_literal()?
            } else if ch == '$' {
                self.dollar_arg()?
            } else {
                self.symbol()?
            };
            tokens.push(Token { kind, line, column });
        }
    }

    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            while self.peek(0).is_some_and(char::is_whitespace) {
                self.advance();
            }
            if self.peek(0) == Some('/') && self.peek(1) == Some('/') {
                while self.peek(0).is_some_and(|ch| ch != '\n') {
                    self.advance();
                }
                continue;
            }
            if self.peek(0) == Some('/') && self.peek(1) == Some('*') {
                self.advance();
                self.advance();
                while !(self.peek(0) == Some('*') && self.peek(1) == Some('/')) {
                    if self.peek(0).is_none() {
                        bail!("line {}: unterminated block comment", self.line);
                    }
                    self.advance();
                }
                self.advance();
                self.advance();
                continue;
            }
            if self.peek(0) == Some('#') {
                while self.peek(0).is_some_and(|ch| ch != '\n') {
                    self.advance();
                }
                continue;
            }
            return Ok(());
        }
    }

    fn identifier(&mut self) -> TokenKind {
        let start = self.offset;
        while self
            .peek(0)
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            self.advance();
        }
        TokenKind::Identifier(self.chars[start..self.offset].iter().collect())
    }

    fn number(&mut self) -> Result<TokenKind> {
        let start = self.offset;
        if self.peek(0) == Some('0') && matches!(self.peek(1), Some('x' | 'X')) {
            self.advance();
            self.advance();
            let digits_start = self.offset;
            while self.peek(0).is_some_and(|ch| ch.is_ascii_hexdigit()) {
                self.advance();
            }
            if digits_start == self.offset {
                bail!("line {}: malformed hexadecimal number", self.line);
            }
            let text: String = self.chars[digits_start..self.offset].iter().collect();
            return Ok(TokenKind::Number(i64::from_str_radix(&text, 16)?));
        }
        while self.peek(0).is_some_and(|ch| ch.is_ascii_digit()) {
            self.advance();
        }
        if self.peek(0) == Some('.') && self.peek(1) != Some('.') {
            self.advance();
            while self.peek(0).is_some_and(|ch| ch.is_ascii_digit()) {
                self.advance();
            }
            let text: String = self.chars[start..self.offset].iter().collect();
            return Ok(TokenKind::Float(text.parse()?));
        }
        let text: String = self.chars[start..self.offset].iter().collect();
        Ok(TokenKind::Number(text.parse()?))
    }

    fn string(&mut self) -> Result<TokenKind> {
        self.advance();
        let mut value = String::new();
        loop {
            match self.advance() {
                Some('"') => return Ok(TokenKind::String(value)),
                Some('\\') => match self.advance() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('0') => value.push('\0'),
                    Some(other) => {
                        value.push('\\');
                        value.push(other);
                    }
                    None => bail!("line {}: unterminated string", self.line),
                },
                Some(ch) => value.push(ch),
                None => bail!("line {}: unterminated string", self.line),
            }
        }
    }

    fn char_literal(&mut self) -> Result<TokenKind> {
        let line = self.line;
        let column = self.column;
        self.advance(); // opening '
        let ch = match self.advance() {
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some('0') => '\0',
                Some(other) => other,
                None => bail!("line {line}, column {column}: unterminated character literal"),
            },
            Some('\'') => bail!("line {line}, column {column}: empty character literal"),
            Some(ch) => ch,
            None => bail!("line {line}, column {column}: unterminated character literal"),
        };
        if self.advance() != Some('\'') {
            bail!("line {line}, column {column}: unterminated character literal");
        }
        Ok(TokenKind::Number(ch as i64))
    }

    fn dollar_arg(&mut self) -> Result<TokenKind> {
        let column = self.column;
        self.advance(); // '$'
        if !self.peek(0).is_some_and(|ch| ch.is_ascii_digit()) {
            bail!(
                "line {}, column {}: expected digits after '$'",
                self.line,
                column
            );
        }
        let start = self.offset;
        while self.peek(0).is_some_and(|ch| ch.is_ascii_digit()) {
            self.advance();
        }
        let text: String = self.chars[start..self.offset].iter().collect();
        let index: usize = match text.parse() {
            Ok(value) => value,
            Err(_) => bail!("line {}: invalid functional argument ${text}", self.line),
        };
        if index == 0 {
            bail!("line {}: functional arguments are 1-based ($1, $2, …)", self.line);
        }
        Ok(TokenKind::DollarArg(index))
    }

    fn symbol(&mut self) -> Result<TokenKind> {
        const DOUBLE: [&str; 17] = [
            "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "|=", "&=", "->", "..", "::",
            "++", "--",
        ];
        // `(:` / `:)` for functionals — but `(::` is `(` + `::`, not `(:` + `:`.
        if self.peek(0) == Some('(')
            && self.peek(1) == Some(':')
            && self.peek(2) != Some(':')
        {
            self.advance();
            self.advance();
            return Ok(TokenKind::Symbol("(:".to_owned()));
        }
        if self.peek(0) == Some(':') && self.peek(1) == Some(')') {
            self.advance();
            self.advance();
            return Ok(TokenKind::Symbol(":)".to_owned()));
        }
        let pair = match (self.peek(0), self.peek(1)) {
            (Some(a), Some(b)) => Some(format!("{a}{b}")),
            _ => None,
        };
        if let Some(pair) = pair.filter(|pair| DOUBLE.contains(&pair.as_str())) {
            self.advance();
            self.advance();
            return Ok(TokenKind::Symbol(pair));
        }
        let Some(ch) = self.advance() else {
            bail!("unexpected end of source");
        };
        if "{}()[];,:.+-*/%!<>=?|&^~".contains(ch) {
            Ok(TokenKind::Symbol(ch.to_string()))
        } else {
            bail!(
                "line {}, column {}: unexpected character {ch:?}",
                self.line,
                self.column.saturating_sub(1)
            )
        }
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.offset + ahead).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek(0)?;
        self.offset += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }
}
