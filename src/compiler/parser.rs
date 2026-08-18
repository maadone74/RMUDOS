use super::ast::{
    BinaryOp, CaseLabel, ClassDecl, Expr, FunctionDecl, PostfixOp, ProgramAst, Stmt, SwitchCase,
    UnaryOp, VariableDecl,
};
use super::lexer::{lex, Token, TokenKind};
use anyhow::{bail, Result};

const TYPE_NAMES: &[&str] = &[
    "void", "mixed", "int", "float", "string", "object", "mapping", "function",
];
const MODIFIERS: &[&str] = &[
    "public", "private", "protected", "static", "nomask", "varargs", "nosave",
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
        let mut classes = Vec::new();
        let mut globals = Vec::new();
        let mut functions = Vec::new();
        while !self.at_eof() {
            if self.consume_identifier("inherit") {
                let path = self.expect_string()?;
                self.expect_symbol(";")?;
                inherits.push(path);
                continue;
            }
            let nosave = self.skip_modifiers_tracking_nosave();
            let line = self.current().line;
            if self.is_class_declaration() {
                classes.push(self.parse_class_decl()?);
                continue;
            }
            if self.is_type_start() {
                self.consume_type()?;
                while self.consume_symbol("*") {}
                let mut name = self.expect_identifier()?;
                if self.consume_symbol("(") {
                    let parameters = self.parse_parameters()?;
                    if self.consume_symbol(";") {
                        // MudOS function prototype — ignore.
                    } else {
                        let body = self.parse_statement()?;
                        functions.push(FunctionDecl {
                            name,
                            parameters,
                            body,
                            line,
                        });
                    }
                } else {
                    loop {
                        let initializer = if self.consume_symbol("=") {
                            Some(self.parse_expression()?)
                        } else {
                            None
                        };
                        globals.push(VariableDecl {
                            name,
                            initializer,
                            nosave,
                            line,
                        });
                        if self.consume_symbol(";") {
                            break;
                        }
                        self.expect_symbol(",")?;
                        while self.consume_symbol("*") {}
                        name = self.expect_identifier()?;
                    }
                }
            } else {
                let name = self.expect_identifier()?;
                self.expect_symbol("(")?;
                let parameters = self.parse_parameters()?;
                if self.consume_symbol(";") {
                    // MudOS function prototype — ignore.
                } else {
                    let body = self.parse_statement()?;
                    functions.push(FunctionDecl {
                        name,
                        parameters,
                        body,
                        line,
                    });
                }
            }
        }
        Ok(ProgramAst {
            inherits,
            classes,
            globals,
            functions,
        })
    }

    fn is_class_declaration(&self) -> bool {
        self.check_identifier("class")
            && self
                .tokens
                .get(self.offset + 1)
                .and_then(identifier)
                .is_some()
            && matches!(
                self.tokens.get(self.offset + 2).map(|token| &token.kind),
                Some(TokenKind::Symbol(symbol)) if symbol == "{"
            )
    }

    fn parse_class_decl(&mut self) -> Result<ClassDecl> {
        let line = self.current().line;
        self.expect_identifier()?; // class
        let name = self.expect_identifier()?;
        self.expect_symbol("{")?;
        let mut fields = Vec::new();
        while !self.consume_symbol("}") {
            if self.at_eof() {
                bail!(self.error("unterminated class"));
            }
            self.skip_modifiers();
            self.consume_type()?;
            loop {
                while self.consume_symbol("*") {}
                fields.push(self.expect_identifier()?);
                if self.consume_symbol(";") {
                    break;
                }
                self.expect_symbol(",")?;
            }
        }
        Ok(ClassDecl { name, fields, line })
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
            if self.is_type_start() {
                self.consume_type()?;
                while self.consume_symbol("*") {}
                if self.check_symbol(",") || self.check_symbol(")") {
                    // K&R-style unnamed typed parameter — ignore.
                } else {
                    parameters.push(self.expect_identifier()?);
                }
            } else {
                parameters.push(self.expect_identifier()?);
            }
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
        if self.consume_identifier("for") {
            self.expect_symbol("(")?;
            let init = if self.check_symbol(";") {
                None
            } else {
                Some(self.parse_expression()?)
            };
            self.expect_symbol(";")?;
            let condition = if self.check_symbol(";") {
                None
            } else {
                Some(self.parse_expression()?)
            };
            self.expect_symbol(";")?;
            let step = if self.check_symbol(")") {
                None
            } else {
                Some(self.parse_expression()?)
            };
            self.expect_symbol(")")?;
            return Ok(Stmt::For {
                init,
                condition,
                step,
                body: Box::new(self.parse_statement()?),
            });
        }
        if self.consume_identifier("foreach") {
            self.expect_symbol("(")?;
            // Optional type names before each variable.
            while self.is_type_name() {
                self.offset += 1;
                while self.consume_symbol("*") {}
            }
            let first = self.expect_identifier()?;
            let mut variables = vec![first];
            if self.consume_symbol(",") {
                while self.is_type_name() {
                    self.offset += 1;
                    while self.consume_symbol("*") {}
                }
                variables.push(self.expect_identifier()?);
            }
            if !self.consume_identifier("in") {
                bail!(self.error("expected 'in' in foreach"));
            }
            let collection = self.parse_expression()?;
            self.expect_symbol(")")?;
            return Ok(Stmt::Foreach {
                variables,
                collection,
                body: Box::new(self.parse_statement()?),
            });
        }
        if self.consume_identifier("switch") {
            self.expect_symbol("(")?;
            let value = self.parse_expression()?;
            self.expect_symbol(")")?;
            self.expect_symbol("{")?;
            let cases = self.parse_switch_cases()?;
            return Ok(Stmt::Switch { value, cases });
        }
        if self.consume_identifier("break") {
            self.expect_symbol(";")?;
            return Ok(Stmt::Break);
        }
        if self.consume_identifier("continue") {
            self.expect_symbol(";")?;
            return Ok(Stmt::Continue);
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
            let nosave = self.skip_modifiers_tracking_nosave();
            let line = self.current().line;
            self.consume_type()?;
            let mut statements = Vec::new();
            loop {
                while self.consume_symbol("*") {}
                let name = self.expect_identifier()?;
                let initializer = if self.consume_symbol("=") {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                statements.push(Stmt::Variable(VariableDecl {
                    name,
                    initializer,
                    nosave,
                    line,
                }));
                if self.consume_symbol(";") {
                    break;
                }
                self.expect_symbol(",")?;
            }
            return Ok(if statements.len() == 1 {
                statements.pop().unwrap()
            } else {
                Stmt::Block(statements)
            });
        }
        let expression = self.parse_expression()?;
        self.expect_symbol(";")?;
        Ok(Stmt::Expression(expression))
    }

    fn parse_switch_cases(&mut self) -> Result<Vec<SwitchCase>> {
        let mut cases = Vec::new();
        while !self.consume_symbol("}") {
            if self.at_eof() {
                bail!(self.error("unterminated switch"));
            }
            let mut labels = Vec::new();
            loop {
                if self.consume_identifier("case") {
                    let start = self.parse_assignment()?;
                    let label = if self.consume_symbol("..") {
                        let end = self.parse_assignment()?;
                        CaseLabel::Range(start, end)
                    } else {
                        CaseLabel::Value(start)
                    };
                    labels.push(Some(label));
                    self.expect_symbol(":")?;
                    continue;
                }
                if self.consume_identifier("default") {
                    labels.push(None);
                    self.expect_symbol(":")?;
                    continue;
                }
                break;
            }
            if labels.is_empty() {
                bail!(self.error("expected case or default in switch"));
            }
            let mut body = Vec::new();
            while !self.check_symbol("}")
                && !self.check_identifier("case")
                && !self.check_identifier("default")
            {
                body.push(self.parse_statement()?);
            }
            cases.push(SwitchCase { labels, body });
        }
        Ok(cases)
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        let mut left = self.parse_assignment()?;
        while self.consume_symbol(",") {
            let right = self.parse_assignment()?;
            left = Expr::Comma {
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_assignment(&mut self) -> Result<Expr> {
        let target = self.parse_conditional()?;
        let operator = ["=", "+=", "-=", "*=", "/=", "|=", "&="]
            .iter()
            .find(|operator| self.check_symbol(operator))
            .copied();
        let Some(operator) = operator else {
            return Ok(target);
        };
        self.offset += 1;
        let right = self.parse_assignment()?;
        // MudOS: `!x = y` means `!(x = y)` (common in if conditions).
        if let Expr::Unary {
            operator: UnaryOp::Not,
            operand,
        } = target
        {
            if is_lvalue(&operand) {
                let assigned = make_assign(operator, *operand, right)?;
                return Ok(Expr::Unary {
                    operator: UnaryOp::Not,
                    operand: Box::new(assigned),
                });
            }
            bail!(self.error("invalid assignment target"));
        }
        // MudOS allows `a && b = c` meaning `a && (b = c)`.
        if let Expr::Binary {
            left: outer_left,
            operator: bin_op @ (BinaryOp::And | BinaryOp::Or),
            right: inner,
        } = target
        {
            if is_lvalue(&inner) {
                let assigned = make_assign(operator, *inner, right)?;
                return Ok(Expr::Binary {
                    left: outer_left,
                    operator: bin_op,
                    right: Box::new(assigned),
                });
            }
            bail!(self.error("invalid assignment target"));
        }
        if !is_lvalue(&target) {
            bail!(self.error("invalid assignment target"));
        }
        make_assign(operator, target, right)
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
        if self.consume_symbol("~") {
            return Ok(Expr::Unary {
                operator: UnaryOp::BitNot,
                operand: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_symbol("++") {
            return Ok(Expr::Unary {
                operator: UnaryOp::PreIncrement,
                operand: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_symbol("--") {
            return Ok(Expr::Unary {
                operator: UnaryOp::PreDecrement,
                operand: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_symbol("-") {
            return Ok(Expr::Unary {
                operator: UnaryOp::Negate,
                operand: Box::new(self.parse_unary()?),
            });
        }
        if self.consume_symbol("*") {
            return Ok(Expr::Unary {
                operator: UnaryOp::Deref,
                operand: Box::new(self.parse_unary()?),
            });
        }
        if self.check_symbol("(") && self.is_cast() {
            self.offset += 1;
            let type_name = if self.consume_identifier("class") {
                let name = self.expect_identifier()?;
                format!("class:{name}")
            } else {
                self.expect_identifier()?
            };
            while self.consume_symbol("*") {}
            self.expect_symbol(")")?;
            return Ok(Expr::Cast {
                type_name,
                value: Box::new(self.parse_unary()?),
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
                    Some(Box::new(self.parse_assignment()?))
                };
                if self.consume_symbol("..") {
                    let end = if self.check_symbol("]") {
                        None
                    } else {
                        Some(Box::new(self.parse_assignment()?))
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
                let member = self.expect_identifier()?;
                if self.consume_symbol("(") {
                    let mut arguments = vec![expression, Expr::String(member)];
                    arguments.extend(self.parse_arguments_after_open()?);
                    expression = Expr::Call {
                        name: "call_other".to_owned(),
                        arguments,
                    };
                } else {
                    expression = Expr::Member {
                        object: Box::new(expression),
                        field: member,
                    };
                }
                continue;
            }
            if self.consume_symbol("(") {
                let arguments = self.parse_arguments_after_open()?;
                expression = Expr::CallValue {
                    callee: Box::new(expression),
                    arguments,
                };
                continue;
            }
            if self.consume_symbol("++") {
                expression = Expr::Postfix {
                    operator: PostfixOp::Increment,
                    operand: Box::new(expression),
                };
                continue;
            }
            if self.consume_symbol("--") {
                expression = Expr::Postfix {
                    operator: PostfixOp::Decrement,
                    operand: Box::new(expression),
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
                let mut value = value;
                while let TokenKind::String(next) = self.current().kind.clone() {
                    self.offset += 1;
                    value.push_str(&next);
                }
                Ok(Expr::String(value))
            }
            TokenKind::StringArray(lines) => {
                self.offset += 1;
                Ok(Expr::Array(
                    lines.into_iter().map(Expr::String).collect(),
                ))
            }
            TokenKind::DollarArg(index) => {
                self.offset += 1;
                Ok(Expr::DollarArg(index))
            }
            TokenKind::Symbol(ref symbol) if symbol == "::" => {
                self.offset += 1;
                let name = self.expect_identifier()?;
                self.expect_symbol("(")?;
                let arguments = self.parse_arguments_after_open()?;
                Ok(Expr::InheritCall {
                    inherit: None,
                    name,
                    arguments,
                })
            }
            TokenKind::Identifier(name) => {
                self.offset += 1;
                if name == "null" {
                    return Ok(Expr::Null);
                }
                if name == "catch" {
                    self.expect_symbol("(")?;
                    let inner = self.parse_expression()?;
                    self.expect_symbol(")")?;
                    return Ok(Expr::Catch(Box::new(inner)));
                }
                if self.consume_symbol("::") {
                    let method = self.expect_identifier()?;
                    self.expect_symbol("(")?;
                    let arguments = self.parse_arguments_after_open()?;
                    return Ok(Expr::InheritCall {
                        inherit: Some(name),
                        name: method,
                        arguments,
                    });
                }
                if self.consume_symbol("(") {
                    if name == "new" && self.check_identifier("class") {
                        self.offset += 1; // class
                        let class_name = self.expect_identifier()?;
                        self.expect_symbol(")")?;
                        return Ok(Expr::NewClass { class_name });
                    }
                    let arguments = self.parse_arguments_after_open()?;
                    Ok(Expr::Call { name, arguments })
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            TokenKind::Symbol(ref symbol) if symbol == "(:" => {
                self.offset += 1;
                self.parse_functional()
            }
            TokenKind::Symbol(ref symbol) if symbol == "[" => {
                self.offset += 1;
                let mut values = Vec::new();
                if self.consume_symbol("]") {
                    return Ok(Expr::Array(values));
                }
                loop {
                    values.push(self.parse_assignment()?);
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
                        let key = self.parse_assignment()?;
                        self.expect_symbol(":")?;
                        let value = self.parse_assignment()?;
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

    fn parse_functional(&mut self) -> Result<Expr> {
        let first = self.parse_assignment()?;
        if self.consume_symbol(",") {
            let Expr::Variable(name) = first else {
                bail!(self.error("functional with bound arguments requires a function name"));
            };
            let mut bound = Vec::new();
            if !self.consume_symbol(":)") {
                loop {
                    bound.push(self.parse_assignment()?);
                    if self.consume_symbol(":)") {
                        break;
                    }
                    self.expect_symbol(",")?;
                    if self.consume_symbol(":)") {
                        break;
                    }
                }
            }
            return Ok(Expr::FunctionalNamed { name, bound });
        }
        self.expect_symbol(":)")?;
        match first {
            Expr::Variable(name) => Ok(Expr::FunctionalNamed {
                name,
                bound: Vec::new(),
            }),
            body => Ok(Expr::FunctionalExpr {
                body: Box::new(body),
            }),
        }
    }

    fn is_cast(&self) -> bool {
        let type_token = self.tokens.get(self.offset + 1);
        let Some(name) = type_token.and_then(identifier) else {
            return false;
        };
        let mut idx = self.offset + 2;
        if name == "class" {
            if self.tokens.get(idx).and_then(identifier).is_none() {
                return false;
            }
            idx += 1;
        } else if !TYPE_NAMES.contains(&name) {
            return false;
        }
        while matches!(
            self.tokens.get(idx).map(|token| &token.kind),
            Some(TokenKind::Symbol(symbol)) if symbol == "*"
        ) {
            idx += 1;
        }
        matches!(
            self.tokens.get(idx).map(|token| &token.kind),
            Some(TokenKind::Symbol(symbol)) if symbol == ")"
        )
    }

    fn parse_arguments_after_open(&mut self) -> Result<Vec<Expr>> {
        let mut arguments = Vec::new();
        if self.consume_symbol(")") {
            return Ok(arguments);
        }
        loop {
            if self.consume_symbol(")") {
                break;
            }
            arguments.push(self.parse_assignment()?);
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
            "|" => (BinaryOp::BitOr, 3),
            "^" => (BinaryOp::BitXor, 4),
            "&" => (BinaryOp::BitAnd, 5),
            "==" => (BinaryOp::Equal, 6),
            "!=" => (BinaryOp::NotEqual, 6),
            "<" => (BinaryOp::Less, 7),
            "<=" => (BinaryOp::LessEqual, 7),
            ">" => (BinaryOp::Greater, 7),
            ">=" => (BinaryOp::GreaterEqual, 7),
            "+" => (BinaryOp::Add, 8),
            "-" => (BinaryOp::Subtract, 8),
            "*" => (BinaryOp::Multiply, 9),
            "/" => (BinaryOp::Divide, 9),
            "%" => (BinaryOp::Modulo, 9),
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
        self.is_type_start_at(offset)
    }

    fn is_type_name(&self) -> bool {
        self.is_type_start()
    }

    fn is_type_start(&self) -> bool {
        self.is_type_start_at(self.offset)
    }

    fn is_type_start_at(&self, offset: usize) -> bool {
        let Some(word) = self.tokens.get(offset).and_then(identifier) else {
            return false;
        };
        if TYPE_NAMES.contains(&word) {
            return true;
        }
        word == "class" && self.tokens.get(offset + 1).and_then(identifier).is_some()
    }

    fn consume_type(&mut self) -> Result<()> {
        if self.consume_identifier("class") {
            self.expect_identifier()?;
            Ok(())
        } else if self
            .current_identifier()
            .is_some_and(|word| TYPE_NAMES.contains(&word))
        {
            self.offset += 1;
            Ok(())
        } else {
            bail!(self.error("expected type"))
        }
    }

    fn skip_modifiers(&mut self) {
        while self
            .current_identifier()
            .is_some_and(|word| MODIFIERS.contains(&word))
        {
            self.offset += 1;
        }
    }

    fn skip_modifiers_tracking_nosave(&mut self) -> bool {
        let mut nosave = false;
        while self
            .current_identifier()
            .is_some_and(|word| MODIFIERS.contains(&word))
        {
            if self.check_identifier("nosave") {
                nosave = true;
            }
            self.offset += 1;
        }
        nosave
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

pub(crate) fn is_lvalue(expression: &Expr) -> bool {
    match expression {
        Expr::Variable(_) | Expr::Index { .. } | Expr::Slice { .. } | Expr::Member { .. } => true,
        Expr::Unary {
            operator: UnaryOp::Deref,
            operand,
        } => is_lvalue(operand),
        _ => false,
    }
}

fn make_assign(operator: &str, target: Expr, right: Expr) -> Result<Expr> {
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
        "|=" => Expr::Binary {
            left: Box::new(target.clone()),
            operator: BinaryOp::BitOr,
            right: Box::new(right),
        },
        "&=" => Expr::Binary {
            left: Box::new(target.clone()),
            operator: BinaryOp::BitAnd,
            right: Box::new(right),
        },
        _ => unreachable!(),
    };
    Ok(Expr::Assign {
        target: Box::new(target),
        value: Box::new(value),
    })
}
