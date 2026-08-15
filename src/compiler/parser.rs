use super::ast::{
    BinaryOp, Expr, FunctionDecl, ProgramAst, Stmt, UnaryOp, VariableDecl,
};
use super::lexer::{lex, Token, TokenKind};
use anyhow::{bail, Result};

const TYPE_NAMES: &[&str] = &[
    "void", "mixed", "int", "float", "string", "object", "mapping", "function",
];
const MODIFIERS: &[&str] = &[
    "public", "private", "protected", "static", "nomask", "varargs",
];

pub fn parse(source: &str) -> Result<ProgramAst> {
    let rewritten = rewrite_array_literals(source);
    Parser::new(lex(&rewritten)?).parse_program()
}

pub fn rewrite_array_literals(source: &str) -> String {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum State {
        Normal,
        String,
        LineComment,
        BlockComment,
    }

    let chars: Vec<char> = source.chars().collect();
    let mut output = String::with_capacity(source.len());
    let mut offset = 0;
    let mut state = State::Normal;
    let mut escaped = false;
    while offset < chars.len() {
        let ch = chars[offset];
        match state {
            State::String => {
                output.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    state = State::Normal;
                }
                offset += 1;
            }
            State::LineComment => {
                output.push(ch);
                offset += 1;
                if ch == '\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                output.push(ch);
                offset += 1;
                if ch == '*' && chars.get(offset) == Some(&'/') {
                    output.push('/');
                    offset += 1;
                    state = State::Normal;
                }
            }
            State::Normal => {
                if ch == '"' {
                    output.push(ch);
                    offset += 1;
                    state = State::String;
                } else if ch == '/' && chars.get(offset + 1) == Some(&'/') {
                    output.push('/');
                    output.push('/');
                    offset += 2;
                    state = State::LineComment;
                } else if ch == '/' && chars.get(offset + 1) == Some(&'*') {
                    output.push('/');
                    output.push('*');
                    offset += 2;
                    state = State::BlockComment;
                } else if ch == '(' {
                    let mut next = offset + 1;
                    while chars.get(next).is_some_and(|ch| ch.is_whitespace()) {
                        next += 1;
                    }
                    if chars.get(next) == Some(&'{') {
                        output.push('[');
                        for whitespace in &chars[offset + 1..next] {
                            output.push(*whitespace);
                        }
                        offset = next + 1;
                    } else {
                        output.push(ch);
                        offset += 1;
                    }
                } else if ch == '}' {
                    let mut next = offset + 1;
                    while chars.get(next).is_some_and(|ch| ch.is_whitespace()) {
                        next += 1;
                    }
                    if chars.get(next) == Some(&')') {
                        output.push(']');
                        for whitespace in &chars[offset + 1..next] {
                            output.push(*whitespace);
                        }
                        offset = next + 1;
                    } else {
                        output.push(ch);
                        offset += 1;
                    }
                } else {
                    output.push(ch);
                    offset += 1;
                }
            }
        }
    }
    output
}

struct Parser {
    tokens: Vec<Token>,
    offset: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, offset: 0 }
    }

    fn parse_program(mut self) -> Result<ProgramAst> {
        let mut inherits = Vec::new();
        let mut globals = Vec::new();
        let mut functions = Vec::new();
        while !self.at_eof() {
            if self.consume_identifier("inherit") {
                let path = self.expect_string()?;
                self.expect_symbol(";")?;
                inherits.push(path);
                continue;
            }
            self.skip_modifiers();
            let line = self.current().line;
            let first = self.expect_identifier()?;
            let (type_name, name) = if self.check_symbol("(") {
                (None, first)
            } else {
                while self.consume_symbol("*") {}
                let name = self.expect_identifier()?;
                (Some(first), name)
            };
            if self.consume_symbol("(") {
                let parameters = self.parse_parameters()?;
                let body = self.parse_statement()?;
                functions.push(FunctionDecl {
                    name,
                    parameters,
                    body,
                    line,
                });
            } else {
                if type_name.is_none() {
                    bail!(self.error("global declaration requires a type"));
                }
                let initializer = if self.consume_symbol("=") {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                self.expect_symbol(";")?;
                globals.push(VariableDecl {
                    name,
                    initializer,
                    line,
                });
            }
        }
        Ok(ProgramAst {
            inherits,
            globals,
            functions,
        })
    }

    fn parse_parameters(&mut self) -> Result<Vec<String>> {
        let mut parameters = Vec::new();
        if self.consume_symbol(")") {
            return Ok(parameters);
        }
        if self.check_identifier("void")
            && self
                .tokens
                .get(self.offset + 1)
                .is_some_and(|token| token.kind == TokenKind::Symbol(")".to_owned()))
        {
            self.offset += 1;
            self.expect_symbol(")")?;
            return Ok(parameters);
        }
        loop {
            self.skip_modifiers();
            let first = self.expect_identifier()?;
            while self.consume_symbol("*") {}
            let name = if self.check_symbol(",") || self.check_symbol(")") {
                first
            } else {
                self.expect_identifier()?
            };
            parameters.push(name);
            if self.consume_symbol(")") {
                break;
            }
            self.expect_symbol(",")?;
        }
        Ok(parameters)
    }

    fn parse_statement(&mut self) -> Result<Stmt> {
        if self.consume_symbol("{") {
            let mut statements = Vec::new();
            while !self.consume_symbol("}") {
                if self.at_eof() {
                    bail!(self.error("unterminated block"));
                }
                statements.push(self.parse_statement()?);
            }
            return Ok(Stmt::Block(statements));
        }
        if self.consume_identifier("if") {
            self.expect_symbol("(")?;
            let condition = self.parse_expression()?;
            self.expect_symbol(")")?;
            let then_branch = Box::new(self.parse_statement()?);
            let else_branch = if self.consume_identifier("else") {
                Some(Box::new(self.parse_statement()?))
            } else {
                None
            };
            return Ok(Stmt::If {
                condition,
                then_branch,
                else_branch,
            });
        }
        if self.consume_identifier("while") {
            self.expect_symbol("(")?;
            let condition = self.parse_expression()?;
            self.expect_symbol(")")?;
            return Ok(Stmt::While {
                condition,
                body: Box::new(self.parse_statement()?),
            });
        }
        if self.consume_identifier("return") {
            if self.consume_symbol(";") {
                return Ok(Stmt::Return(None));
            }
            let value = self.parse_expression()?;
            self.expect_symbol(";")?;
            return Ok(Stmt::Return(Some(value)));
        }
        if self.consume_symbol(";") {
            return Ok(Stmt::Empty);
        }
        if self.is_declaration() {
            self.skip_modifiers();
            let line = self.current().line;
            self.expect_identifier()?;
            while self.consume_symbol("*") {}
            let name = self.expect_identifier()?;
            let initializer = if self.consume_symbol("=") {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.expect_symbol(";")?;
            return Ok(Stmt::Variable(VariableDecl {
                name,
                initializer,
                line,
            }));
        }
        let expression = self.parse_expression()?;
        self.expect_symbol(";")?;
        Ok(Stmt::Expression(expression))
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let target = self.parse_conditional()?;
        let operator = ["=", "+=", "-=", "*=", "/="]
            .iter()
            .find(|operator| self.check_symbol(operator))
            .copied();
        let Some(operator) = operator else {
            return Ok(target);
        };
        self.offset += 1;
        let right = self.parse_assignment()?;
        let value = match operator {
            "=" => right,
            "+=" => Expr::Binary {
                left: Box::new(target.clone()),
                operator: BinaryOp::Add,
                right: Box::new(right),
            },
            "-=" => Expr::Binary {
                left: Box::new(target.clone()),
                operator: BinaryOp::Subtract,
                right: Box::new(right),
            },
            "*=" => Expr::Binary {
                left: Box::new(target.clone()),
                operator: BinaryOp::Multiply,
                right: Box::new(right),
            },
            "/=" => Expr::Binary {
                left: Box::new(target.clone()),
                operator: BinaryOp::Divide,
                right: Box::new(right),
            },
            _ => unreachable!(),
        };
        Ok(Expr::Assign {
            target: Box::new(target),
            value: Box::new(value),
        })
    }

    fn parse_conditional(&mut self) -> Result<Expr> {
        let condition = self.parse_binary(1)?;
        if !self.consume_symbol("?") {
            return Ok(condition);
        }
        let then_value = self.parse_expression()?;
        self.expect_symbol(":")?;
        let else_value = self.parse_conditional()?;
        Ok(Expr::Conditional {
            condition: Box::new(condition),
            then_value: Box::new(then_value),
            else_value: Box::new(else_value),
        })
    }

    fn parse_binary(&mut self, minimum_precedence: u8) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        loop {
            let Some((operator, precedence)) = self.binary_operator() else {
                break;
            };
            if precedence < minimum_precedence {
                break;
            }
            self.offset += 1;
            let right = self.parse_binary(precedence + 1)?;
            left = Expr::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if self.consume_symbol("!") {
            return Ok(Expr::Unary {
                operator: UnaryOp::Not,
                operand: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_symbol("-") {
            return Ok(Expr::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(self.parse_unary()?),
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.consume_symbol("[") {
                let start = if self.check_symbol("..") || self.check_symbol("]") {
                    None
                } else {
                    Some(Box::new(self.parse_expression()?))
                };
                if self.consume_symbol("..") {
                    let end = if self.check_symbol("]") {
                        None
                    } else {
                        Some(Box::new(self.parse_expression()?))
                    };
                    self.expect_symbol("]")?;
                    expression = Expr::Slice {
                        value: Box::new(expression),
                        start,
                        end,
                    };
                } else {
                    self.expect_symbol("]")?;
                    let Some(index) = start else {
                        bail!(self.error("array index may not be empty"));
                    };
                    expression = Expr::Index {
                        value: Box::new(expression),
                        index,
                    };
                }
                continue;
            }
            if self.consume_symbol("->") {
                let function = self.expect_identifier()?;
                self.expect_symbol("(")?;
                let mut arguments = vec![expression, Expr::String(function)];
                arguments.extend(self.parse_arguments_after_open()?);
                expression = Expr::Call {
                    name: "call_other".to_owned(),
                    arguments,
                };
                continue;
            }
            break;
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(value) => {
                self.offset += 1;
                Ok(Expr::Int(value))
            }
            TokenKind::Float(value) => {
                self.offset += 1;
                Ok(Expr::Float(value))
            }
            TokenKind::String(value) => {
                self.offset += 1;
                Ok(Expr::String(value))
            }
            TokenKind::Identifier(name) => {
                self.offset += 1;
                if name == "null" {
                    return Ok(Expr::Null);
                }
                if self.consume_symbol("(") {
                    let arguments = self.parse_arguments_after_open()?;
                    Ok(Expr::Call { name, arguments })
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            TokenKind::Symbol(ref symbol) if symbol == "[" => {
                self.offset += 1;
                let mut values = Vec::new();
                if self.consume_symbol("]") {
                    return Ok(Expr::Array(values));
                }
                loop {
                    values.push(self.parse_expression()?);
                    if self.consume_symbol("]") {
                        break;
                    }
                    self.expect_symbol(",")?;
                    if self.consume_symbol("]") {
                        break;
                    }
                }
                Ok(Expr::Array(values))
            }
            TokenKind::Symbol(ref symbol) if symbol == "(" => {
                self.offset += 1;
                if self.consume_symbol("[") {
                    let mut entries = Vec::new();
                    if self.consume_symbol("]") {
                        self.expect_symbol(")")?;
                        return Ok(Expr::Mapping(entries));
                    }
                    loop {
                        let key = self.parse_expression()?;
                        self.expect_symbol(":")?;
                        let value = self.parse_expression()?;
                        entries.push((key, value));
                        if self.consume_symbol("]") {
                            break;
                        }
                        self.expect_symbol(",")?;
                        if self.consume_symbol("]") {
                            break;
                        }
                    }
                    self.expect_symbol(")")?;
                    Ok(Expr::Mapping(entries))
                } else {
                    let expression = self.parse_expression()?;
                    self.expect_symbol(")")?;
                    Ok(expression)
                }
            }
            _ => bail!(self.error("expected expression")),
        }
    }

    fn parse_arguments_after_open(&mut self) -> Result<Vec<Expr>> {
        let mut arguments = Vec::new();
        if self.consume_symbol(")") {
            return Ok(arguments);
        }
        loop {
            arguments.push(self.parse_expression()?);
            if self.consume_symbol(")") {
                break;
            }
            self.expect_symbol(",")?;
        }
        Ok(arguments)
    }

    fn binary_operator(&self) -> Option<(BinaryOp, u8)> {
        let TokenKind::Symbol(symbol) = &self.current().kind else {
            return None;
        };
        Some(match symbol.as_str() {
            "||" => (BinaryOp::Or, 1),
            "&&" => (BinaryOp::And, 2),
            "==" => (BinaryOp::Equal, 3),
            "!=" => (BinaryOp::NotEqual, 3),
            "<" => (BinaryOp::Less, 4),
            "<=" => (BinaryOp::LessEqual, 4),
            ">" => (BinaryOp::Greater, 4),
            ">=" => (BinaryOp::GreaterEqual, 4),
            "+" => (BinaryOp::Add, 5),
            "-" => (BinaryOp::Subtract, 5),
            "*" => (BinaryOp::Multiply, 6),
            "/" => (BinaryOp::Divide, 6),
            "%" => (BinaryOp::Modulo, 6),
            _ => return None,
        })
    }

    fn is_declaration(&self) -> bool {
        let mut offset = self.offset;
        while self
            .tokens
            .get(offset)
            .and_then(identifier)
            .is_some_and(|word| MODIFIERS.contains(&word))
        {
            offset += 1;
        }
        self.tokens
            .get(offset)
            .and_then(identifier)
            .is_some_and(|word| TYPE_NAMES.contains(&word))
    }

    fn skip_modifiers(&mut self) {
        while self
            .current_identifier()
            .is_some_and(|word| MODIFIERS.contains(&word))
        {
            self.offset += 1;
        }
    }

    fn expect_identifier(&mut self) -> Result<String> {
        match self.current().kind.clone() {
            TokenKind::Identifier(value) => {
                self.offset += 1;
                Ok(value)
            }
            _ => bail!(self.error("expected identifier")),
        }
    }

    fn expect_string(&mut self) -> Result<String> {
        match self.current().kind.clone() {
            TokenKind::String(value) => {
                self.offset += 1;
                Ok(value)
            }
            _ => bail!(self.error("expected string")),
        }
    }

    fn expect_symbol(&mut self, expected: &str) -> Result<()> {
        if self.consume_symbol(expected) {
            Ok(())
        } else {
            bail!(self.error(&format!("expected {expected:?}")))
        }
    }

    fn consume_symbol(&mut self, expected: &str) -> bool {
        if self.check_symbol(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn check_symbol(&self, expected: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Symbol(value) if value == expected)
    }

    fn consume_identifier(&mut self, expected: &str) -> bool {
        if self.check_identifier(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn check_identifier(&self, expected: &str) -> bool {
        self.current_identifier() == Some(expected)
    }

    fn current_identifier(&self) -> Option<&str> {
        identifier(self.current())
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.offset.min(self.tokens.len() - 1)]
    }

    fn error(&self, message: &str) -> String {
        format!(
            "line {}, column {}: {message}",
            self.current().line,
            self.current().column
        )
    }
}

fn identifier(token: &Token) -> Option<&str> {
    match &token.kind {
        TokenKind::Identifier(value) => Some(value),
        _ => None,
    }
}
