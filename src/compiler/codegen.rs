use super::ast::{BinaryOp, Expr, FunctionDecl, ProgramAst, Stmt, UnaryOp};
use crate::vm::program::{FunctionInfo, Op, Program};
use crate::vm::value::LpcValue;
use anyhow::{bail, Result};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub fn generate(
    ast: ProgramAst,
    path: &str,
    inherited: Vec<Arc<Program>>,
) -> Result<Program> {
    let mut globals = Vec::new();
    for program in &inherited {
        for name in &program.globals {
            if !globals.contains(name) {
                globals.push(name.clone());
            }
        }
    }
    for declaration in &ast.globals {
        if globals.contains(&declaration.name) {
            bail!(
                "{}:{}: duplicate global variable {}",
                path,
                declaration.line,
                declaration.name
            );
        }
        globals.push(declaration.name.clone());
    }

    let callable_names = inherit_fn_names(&inherited, &ast);
    let mut functions = IndexMap::new();
    for program in &inherited {
        let relocation: Vec<usize> = program
            .globals
            .iter()
            .map(|name| {
                globals
                    .iter()
                    .position(|candidate| candidate == name)
                    .expect("inherited global was merged")
            })
            .collect();
        for (name, function) in &program.functions {
            functions
                .entry(name.clone())
                .or_insert_with(|| relocate_function(function, &relocation));
        }
    }

    let mut local_functions = IndexMap::new();
    for declaration in &ast.functions {
        let function = FunctionCompiler::new(&globals, &callable_names, declaration).compile()?;
        functions.insert(declaration.name.clone(), function.clone());
        local_functions.insert(declaration.name.clone(), function);
    }

    let mut initialization = Vec::new();
    {
        let mut compiler = ExpressionCompiler {
            globals: &globals,
            locals: HashMap::new(),
            callable_names: &callable_names,
            code: &mut initialization,
        };
        for declaration in &ast.globals {
            if let Some(initializer) = &declaration.initializer {
                compiler.compile_expression(initializer)?;
                let index = globals
                    .iter()
                    .position(|name| name == &declaration.name)
                    .expect("own global was merged");
                compiler.code.push(Op::StoreGlobal(index));
                compiler.code.push(Op::Pop);
            }
        }
    }
    if !initialization.is_empty() {
        if let Some(create) = local_functions.get_mut("create") {
            let mut code = initialization;
            code.append(&mut create.code);
            create.code = code;
            functions.insert("create".to_owned(), create.clone());
        } else {
            initialization.push(Op::Constant(LpcValue::Null));
            initialization.push(Op::Return);
            let create = FunctionInfo {
                name: "create".to_owned(),
                parameters: Vec::new(),
                local_count: 0,
                code: initialization,
                source_line: 1,
            };
            functions.insert("create".to_owned(), create.clone());
            local_functions.insert("create".to_owned(), create);
        }
    }

    Ok(Program {
        path: path.to_owned(),
        inherits: ast.inherits,
        inherit_programs: inherited,
        globals,
        functions,
        local_functions,
    })
}

pub fn inherit_fn_names(inherited: &[Arc<Program>], ast: &ProgramAst) -> HashSet<String> {
    let mut names = HashSet::new();
    for program in inherited {
        names.extend(program.functions.keys().cloned());
    }
    names.extend(ast.functions.iter().map(|function| function.name.clone()));
    names
}

fn relocate_function(function: &FunctionInfo, relocation: &[usize]) -> FunctionInfo {
    let mut function = function.clone();
    for operation in &mut function.code {
        match operation {
            Op::LoadGlobal(index) | Op::StoreGlobal(index) => {
                *index = relocation[*index];
            }
            _ => {}
        }
    }
    function
}

struct FunctionCompiler<'a> {
    globals: &'a [String],
    callable_names: &'a HashSet<String>,
    declaration: &'a FunctionDecl,
    locals: HashMap<String, usize>,
    code: Vec<Op>,
}

impl<'a> FunctionCompiler<'a> {
    fn new(
        globals: &'a [String],
        callable_names: &'a HashSet<String>,
        declaration: &'a FunctionDecl,
    ) -> Self {
        let locals = declaration
            .parameters
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect();
        Self {
            globals,
            callable_names,
            declaration,
            locals,
            code: Vec::new(),
        }
    }

    fn compile(mut self) -> Result<FunctionInfo> {
        self.compile_statement(&self.declaration.body)?;
        if !matches!(self.code.last(), Some(Op::Return)) {
            self.code.push(Op::Constant(LpcValue::Null));
            self.code.push(Op::Return);
        }
        Ok(FunctionInfo {
            name: self.declaration.name.clone(),
            parameters: self.declaration.parameters.clone(),
            local_count: self.locals.len(),
            code: self.code,
            source_line: self.declaration.line,
        })
    }

    fn compile_statement(&mut self, statement: &Stmt) -> Result<()> {
        match statement {
            Stmt::Block(statements) => {
                for statement in statements {
                    self.compile_statement(statement)?;
                }
            }
            Stmt::Variable(declaration) => {
                if self.locals.contains_key(&declaration.name) {
                    bail!(
                        "line {}: duplicate local variable {}",
                        declaration.line,
                        declaration.name
                    );
                }
                let index = self.locals.len();
                self.locals.insert(declaration.name.clone(), index);
                if let Some(initializer) = &declaration.initializer {
                    self.compile_expression(initializer)?;
                    self.code.push(Op::StoreLocal(index));
                    self.code.push(Op::Pop);
                }
            }
            Stmt::Expression(expression) => {
                self.compile_expression(expression)?;
                self.code.push(Op::Pop);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.compile_expression(condition)?;
                let false_jump = self.code.len();
                self.code.push(Op::JumpIfFalse(usize::MAX));
                self.compile_statement(then_branch)?;
                if let Some(else_branch) = else_branch {
                    let end_jump = self.code.len();
                    self.code.push(Op::Jump(usize::MAX));
                    let else_start = self.code.len();
                    self.patch_jump(false_jump, else_start);
                    self.compile_statement(else_branch)?;
                    let end = self.code.len();
                    self.patch_jump(end_jump, end);
                } else {
                    let end = self.code.len();
                    self.patch_jump(false_jump, end);
                }
            }
            Stmt::While { condition, body } => {
                let start = self.code.len();
                self.compile_expression(condition)?;
                let end_jump = self.code.len();
                self.code.push(Op::JumpIfFalse(usize::MAX));
                self.compile_statement(body)?;
                self.code.push(Op::Jump(start));
                let end = self.code.len();
                self.patch_jump(end_jump, end);
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.compile_expression(value)?;
                } else {
                    self.code.push(Op::Constant(LpcValue::Null));
                }
                self.code.push(Op::Return);
            }
            Stmt::Empty => {}
        }
        Ok(())
    }

    fn compile_expression(&mut self, expression: &Expr) -> Result<()> {
        let mut compiler = ExpressionCompiler {
            globals: self.globals,
            locals: self.locals.clone(),
            callable_names: self.callable_names,
            code: &mut self.code,
        };
        compiler.compile_expression(expression)
    }

    fn patch_jump(&mut self, offset: usize, target: usize) {
        match &mut self.code[offset] {
            Op::Jump(value) | Op::JumpIfFalse(value) => *value = target,
            _ => unreachable!("attempted to patch a non-jump operation"),
        }
    }
}

struct ExpressionCompiler<'a> {
    globals: &'a [String],
    locals: HashMap<String, usize>,
    callable_names: &'a HashSet<String>,
    code: &'a mut Vec<Op>,
}

impl ExpressionCompiler<'_> {
    fn compile_expression(&mut self, expression: &Expr) -> Result<()> {
        match expression {
            Expr::Null => self.code.push(Op::Constant(LpcValue::Null)),
            Expr::Int(value) => self.code.push(Op::Constant(LpcValue::Int(*value))),
            Expr::Float(value) => self.code.push(Op::Constant(LpcValue::Float(*value))),
            Expr::String(value) => self
                .code
                .push(Op::Constant(LpcValue::String(value.clone()))),
            Expr::Array(values) => {
                for value in values {
                    self.compile_expression(value)?;
                }
                self.code.push(Op::MakeArray(values.len()));
            }
            Expr::Mapping(entries) => {
                for (key, value) in entries {
                    self.compile_expression(key)?;
                    self.compile_expression(value)?;
                }
                self.code.push(Op::MakeMapping(entries.len()));
            }
            Expr::Variable(name) => self.load_variable(name)?,
            Expr::Assign { target, value } => {
                match target.as_ref() {
                    Expr::Variable(name) => {
                        self.compile_expression(value)?;
                        self.store_variable(name)?;
                    }
                    Expr::Index { value: base, index } => {
                        self.compile_expression(base)?;
                        self.compile_expression(index)?;
                        self.compile_expression(value)?;
                        self.code.push(Op::IndexSet);
                        if let Expr::Variable(name) = base.as_ref() {
                            self.store_variable(name.as_str())?;
                        }
                    }
                    _ => bail!("assignment targets must be variables"),
                }
            }
            Expr::Unary { operator, operand } => {
                self.compile_expression(operand)?;
                self.code.push(match operator {
                    UnaryOp::Negate => Op::Negate,
                    UnaryOp::Not => Op::Not,
                });
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                self.compile_expression(left)?;
                self.compile_expression(right)?;
                self.code.push(match operator {
                    BinaryOp::Add => Op::Add,
                    BinaryOp::Subtract => Op::Subtract,
                    BinaryOp::Multiply => Op::Multiply,
                    BinaryOp::Divide => Op::Divide,
                    BinaryOp::Modulo => Op::Modulo,
                    BinaryOp::Equal => Op::Equal,
                    BinaryOp::NotEqual => Op::NotEqual,
                    BinaryOp::Less => Op::Less,
                    BinaryOp::LessEqual => Op::LessEqual,
                    BinaryOp::Greater => Op::Greater,
                    BinaryOp::GreaterEqual => Op::GreaterEqual,
                    BinaryOp::And => Op::And,
                    BinaryOp::Or => Op::Or,
                });
            }
            Expr::Conditional {
                condition,
                then_value,
                else_value,
            } => {
                self.compile_expression(condition)?;
                let false_jump = self.code.len();
                self.code.push(Op::JumpIfFalse(usize::MAX));
                self.compile_expression(then_value)?;
                let end_jump = self.code.len();
                self.code.push(Op::Jump(usize::MAX));
                let else_start = self.code.len();
                patch_expression_jump(self.code, false_jump, else_start);
                self.compile_expression(else_value)?;
                let end = self.code.len();
                patch_expression_jump(self.code, end_jump, end);
            }
            Expr::Call { name, arguments } => {
                if name == "this_object" {
                    if !arguments.is_empty() {
                        bail!("this_object() takes no arguments");
                    }
                    self.code.push(Op::ThisObject);
                } else {
                    for argument in arguments {
                        self.compile_expression(argument)?;
                    }
                    if self.callable_names.contains(name) {
                        self.code.push(Op::Call(name.clone(), arguments.len()));
                    } else {
                        self.code
                            .push(Op::CallEfun(name.clone(), arguments.len()));
                    }
                }
            }
            Expr::Index { value, index } => {
                self.compile_expression(value)?;
                self.compile_expression(index)?;
                self.code.push(Op::Index);
            }
            Expr::Slice { value, start, end } => {
                self.compile_expression(value)?;
                if let Some(start) = start {
                    self.compile_expression(start)?;
                } else {
                    self.code.push(Op::Constant(LpcValue::Null));
                }
                if let Some(end) = end {
                    self.compile_expression(end)?;
                } else {
                    self.code.push(Op::Constant(LpcValue::Null));
                }
                self.code.push(Op::Slice);
            }
        }
        Ok(())
    }

    fn load_variable(&mut self, name: &str) -> Result<()> {
        if let Some(index) = self.locals.get(name) {
            self.code.push(Op::LoadLocal(*index));
            return Ok(());
        }
        if let Some(index) = self.globals.iter().position(|candidate| candidate == name) {
            self.code.push(Op::LoadGlobal(index));
            return Ok(());
        }
        bail!("unknown variable {name}")
    }

    fn store_variable(&mut self, name: &str) -> Result<()> {
        if let Some(index) = self.locals.get(name) {
            self.code.push(Op::StoreLocal(*index));
            return Ok(());
        }
        if let Some(index) = self.globals.iter().position(|candidate| candidate == name) {
            self.code.push(Op::StoreGlobal(index));
            return Ok(());
        }
        bail!("unknown assignment target {name}")
    }
}

fn patch_expression_jump(code: &mut [Op], offset: usize, target: usize) {
    match &mut code[offset] {
        Op::Jump(value) | Op::JumpIfFalse(value) => *value = target,
        _ => unreachable!("attempted to patch a non-jump operation"),
    }
}
