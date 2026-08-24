use super::ast::{
    BinaryOp, CaseLabel, Expr, FunctionDecl, PostfixOp, ProgramAst, Stmt, UnaryOp,
};
use super::parser::is_lvalue;
use crate::vm::program::{relocate_function, FunctionInfo, Op, Program};
use crate::vm::value::{ClassDef, LpcValue};
use anyhow::{bail, Result};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct LoopContext {
    continue_target: Option<usize>,
    break_patches: Vec<usize>,
    continue_patches: Vec<usize>,
}

enum Breakable {
    Loop(LoopContext),
    Switch { break_patches: Vec<usize> },
}

pub fn generate(
    ast: ProgramAst,
    path: &str,
    inherited: Vec<Arc<Program>>,
) -> Result<Program> {
    let mut globals = Vec::new();
    let mut nosave_globals = Vec::new();
    for program in &inherited {
        for (index, name) in program.globals.iter().enumerate() {
            if !globals.contains(name) {
                globals.push(name.clone());
                nosave_globals.push(
                    program
                        .nosave_globals
                        .get(index)
                        .copied()
                        .unwrap_or(false),
                );
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
        nosave_globals.push(declaration.nosave);
    }

    let mut classes = IndexMap::new();
    for program in &inherited {
        for (name, def) in &program.classes {
            classes.entry(name.clone()).or_insert_with(|| def.clone());
        }
    }
    for declaration in &ast.classes {
        if classes.contains_key(&declaration.name) {
            bail!(
                "{}:{}: duplicate class {}",
                path,
                declaration.line,
                declaration.name
            );
        }
        classes.insert(
            declaration.name.clone(),
            Arc::new(ClassDef {
                name: declaration.name.clone(),
                fields: declaration.fields.clone(),
            }),
        );
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
            // Later inherits override earlier ones (MudOS semantics).
            functions.insert(
                name.clone(),
                relocate_function(function, &relocation).expect("inherited global was merged"),
            );
        }
    }

    let mut local_functions = IndexMap::new();
    for declaration in &ast.functions {
        let function =
            FunctionCompiler::new(path, &globals, &callable_names, &classes, declaration)
                .compile()?;
        functions.insert(declaration.name.clone(), function.clone());
        local_functions.insert(declaration.name.clone(), function);
    }

    let mut initialization = Vec::new();
    {
        let mut compiler = ExpressionCompiler {
            defining_path: path,
            globals: &globals,
            locals: HashMap::new(),
            callable_names: &callable_names,
            classes: &classes,
            code: &mut initialization,
            allow_dollar_args: false,
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
            let offset = initialization.len();
            // Jump / catch targets are absolute indices into create.code.
            // Prepending global initializers must shift those targets.
            for op in &mut create.code {
                match op {
                    Op::Jump(target) | Op::JumpIfFalse(target) | Op::EnterCatch(target) => {
                        *target = target.saturating_add(offset);
                    }
                    _ => {}
                }
            }
            let mut code = initialization;
            code.append(&mut create.code);
            create.code = code;
            functions.insert("create".to_owned(), create.clone());
        } else {
            initialization.push(Op::Constant(LpcValue::Int(0)));
            initialization.push(Op::Return);
            let create = FunctionInfo {
                name: "create".to_owned(),
                parameters: Vec::new(),
                local_count: 0,
                code: initialization,
                source_line: 1,
                defining_path: path.to_owned(),
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
        nosave_globals,
        classes,
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

struct FunctionCompiler<'a> {
    defining_path: &'a str,
    globals: &'a [String],
    callable_names: &'a HashSet<String>,
    classes: &'a IndexMap<String, Arc<ClassDef>>,
    declaration: &'a FunctionDecl,
    locals: HashMap<String, usize>,
    code: Vec<Op>,
    breakables: Vec<Breakable>,
}

impl<'a> FunctionCompiler<'a> {
    fn new(
        defining_path: &'a str,
        globals: &'a [String],
        callable_names: &'a HashSet<String>,
        classes: &'a IndexMap<String, Arc<ClassDef>>,
        declaration: &'a FunctionDecl,
    ) -> Self {
        let locals = declaration
            .parameters
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect();
        Self {
            defining_path,
            globals,
            callable_names,
            classes,
            declaration,
            locals,
            code: Vec::new(),
            breakables: Vec::new(),
        }
    }

    fn compile(mut self) -> Result<FunctionInfo> {
        self.compile_statement(&self.declaration.body)?;
        if !matches!(self.code.last(), Some(Op::Return)) {
            // MudOS: falling off a function returns 0, not "undefined".
            self.code.push(Op::Constant(LpcValue::Int(0)));
            self.code.push(Op::Return);
        }
        Ok(FunctionInfo {
            name: self.declaration.name.clone(),
            parameters: self.declaration.parameters.clone(),
            local_count: self.locals.len(),
            code: self.code,
            source_line: self.declaration.line,
            defining_path: self.defining_path.to_owned(),
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
                self.breakables.push(Breakable::Loop(LoopContext {
                    continue_target: Some(start),
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                }));
                self.compile_expression(condition)?;
                let end_jump = self.code.len();
                self.code.push(Op::JumpIfFalse(usize::MAX));
                self.compile_statement(body)?;
                self.code.push(Op::Jump(start));
                let end = self.code.len();
                self.patch_jump(end_jump, end);
                self.finish_loop(end);
            }
            Stmt::For {
                init,
                condition,
                step,
                body,
            } => {
                if let Some(init) = init {
                    self.compile_expression(init)?;
                    self.code.push(Op::Pop);
                }
                let loop_start = self.code.len();
                self.breakables.push(Breakable::Loop(LoopContext {
                    continue_target: None,
                    break_patches: Vec::new(),
                    continue_patches: Vec::new(),
                }));
                if let Some(condition) = condition {
                    self.compile_expression(condition)?;
                } else {
                    self.code.push(Op::Constant(LpcValue::Int(1)));
                }
                let end_jump = self.code.len();
                self.code.push(Op::JumpIfFalse(usize::MAX));
                self.compile_statement(body)?;
                let continue_target = self.code.len();
                if let Some(Breakable::Loop(ctx)) = self.breakables.last_mut() {
                    ctx.continue_target = Some(continue_target);
                    let patches = std::mem::take(&mut ctx.continue_patches);
                    for patch in patches {
                        self.patch_jump(patch, continue_target);
                    }
                }
                if let Some(step) = step {
                    self.compile_expression(step)?;
                    self.code.push(Op::Pop);
                }
                self.code.push(Op::Jump(loop_start));
                let end = self.code.len();
                self.patch_jump(end_jump, end);
                self.finish_loop(end);
            }
            Stmt::Foreach {
                variables,
                collection,
                body,
            } => {
                self.compile_foreach(variables, collection, body)?;
            }
            Stmt::Switch { value, cases } => {
                self.compile_expression(value)?;
                self.breakables.push(Breakable::Switch {
                    break_patches: Vec::new(),
                });
                let mut next_case_patches: Vec<usize> = Vec::new();
                let mut after_body_patches: Vec<usize> = Vec::new();
                // MudOS: `default` matches only after every `case` misses.
                // `case 0: default: foo(); case 1: bar();` must still run
                // `bar()` when the value is 1 (armour/weapon virtual_setup).
                let mut default_body: Option<usize> = None;
                for (index, case) in cases.iter().enumerate() {
                    for patch in next_case_patches.drain(..) {
                        self.patch_jump(patch, self.code.len());
                    }
                    let mut body_patches = Vec::new();
                    let mut is_default = false;
                    for label in &case.labels {
                        match label {
                            Some(CaseLabel::Value(label_expr)) => {
                                self.code.push(Op::Dup);
                                self.compile_expression(label_expr)?;
                                self.code.push(Op::Equal);
                                let miss = self.code.len();
                                self.code.push(Op::JumpIfFalse(usize::MAX));
                                let hit = self.code.len();
                                self.code.push(Op::Jump(usize::MAX));
                                body_patches.push(hit);
                                self.patch_jump(miss, self.code.len());
                            }
                            Some(CaseLabel::Range(start, end)) => {
                                // !(value < start) && !(value > end)
                                self.code.push(Op::Dup);
                                self.compile_expression(start)?;
                                self.code.push(Op::Less);
                                let below = self.code.len();
                                self.code.push(Op::JumpIfFalse(usize::MAX)); // not less → continue
                                let miss_lo = self.code.len();
                                self.code.push(Op::Jump(usize::MAX));
                                self.patch_jump(below, self.code.len());
                                self.code.push(Op::Dup);
                                self.compile_expression(end)?;
                                self.code.push(Op::Greater);
                                let above = self.code.len();
                                self.code.push(Op::JumpIfFalse(usize::MAX)); // not greater → hit
                                let miss_hi = self.code.len();
                                self.code.push(Op::Jump(usize::MAX));
                                self.patch_jump(above, self.code.len());
                                let hit = self.code.len();
                                self.code.push(Op::Jump(usize::MAX));
                                body_patches.push(hit);
                                self.patch_jump(miss_lo, self.code.len());
                                self.patch_jump(miss_hi, self.code.len());
                            }
                            None => {
                                is_default = true;
                            }
                        }
                    }
                    let miss_all = self.code.len();
                    self.code.push(Op::Jump(usize::MAX));
                    next_case_patches.push(miss_all);
                    let body_start = self.code.len();
                    if is_default {
                        default_body = Some(body_start);
                    }
                    for patch in body_patches {
                        self.patch_jump(patch, body_start);
                    }
                    for statement in &case.body {
                        self.compile_statement(statement)?;
                    }
                    // No cross-case fallthrough into the next case's tests.
                    if index + 1 < cases.len() {
                        let skip = self.code.len();
                        self.code.push(Op::Jump(usize::MAX));
                        after_body_patches.push(skip);
                    }
                }
                let after_tests = self.code.len();
                let missed_target = default_body.unwrap_or(after_tests);
                for patch in next_case_patches {
                    self.patch_jump(patch, missed_target);
                }
                let cleanup = self.code.len();
                for patch in after_body_patches {
                    self.patch_jump(patch, cleanup);
                }
                if let Some(Breakable::Switch { break_patches }) = self.breakables.pop() {
                    for patch in break_patches {
                        self.patch_jump(patch, cleanup);
                    }
                }
                self.code.push(Op::Pop);
            }
            Stmt::Break => {
                let Some(breakable) = self.breakables.last_mut() else {
                    bail!("break outside of loop or switch");
                };
                let jump = self.code.len();
                self.code.push(Op::Jump(usize::MAX));
                match breakable {
                    Breakable::Loop(ctx) => ctx.break_patches.push(jump),
                    Breakable::Switch { break_patches } => break_patches.push(jump),
                }
            }
            Stmt::Continue => {
                let Some(ctx) = self
                    .breakables
                    .iter_mut()
                    .rev()
                    .find_map(|item| match item {
                        Breakable::Loop(ctx) => Some(ctx),
                        Breakable::Switch { .. } => None,
                    })
                else {
                    bail!("continue outside of loop");
                };
                if let Some(target) = ctx.continue_target {
                    self.code.push(Op::Jump(target));
                } else {
                    let jump = self.code.len();
                    self.code.push(Op::Jump(usize::MAX));
                    ctx.continue_patches.push(jump);
                }
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.compile_expression(value)?;
                } else {
                    // MudOS: `return;` yields 0.
                    self.code.push(Op::Constant(LpcValue::Int(0)));
                }
                self.code.push(Op::Return);
            }
            Stmt::Empty => {}
        }
        Ok(())
    }

    fn finish_loop(&mut self, end: usize) {
        let Some(Breakable::Loop(ctx)) = self.breakables.pop() else {
            panic!("expected loop context");
        };
        for patch in ctx.break_patches {
            self.patch_jump(patch, end);
        }
        for patch in ctx.continue_patches {
            let target = ctx.continue_target.expect("continue target");
            self.patch_jump(patch, target);
        }
    }

    fn ensure_local(&mut self, name: &str) -> usize {
        if let Some(index) = self.locals.get(name) {
            return *index;
        }
        let index = self.locals.len();
        self.locals.insert(name.to_owned(), index);
        index
    }

    fn compile_foreach(
        &mut self,
        variables: &[String],
        collection: &Expr,
        body: &Stmt,
    ) -> Result<()> {
        if variables.is_empty() || variables.len() > 2 {
            bail!("foreach requires one or two variables");
        }
        let coll_tmp = self.ensure_local(&format!("__foreach_coll_{}", self.locals.len()));
        let idx_tmp = self.ensure_local(&format!("__foreach_i_{}", self.locals.len()));
        let len_tmp = self.ensure_local(&format!("__foreach_n_{}", self.locals.len()));
        let keys_tmp = if variables.len() == 2 {
            Some(self.ensure_local(&format!("__foreach_keys_{}", self.locals.len())))
        } else {
            None
        };

        // coll = collection
        self.compile_expression(collection)?;
        self.code.push(Op::StoreLocal(coll_tmp));
        self.code.push(Op::Pop);

        if let Some(keys_tmp) = keys_tmp {
            // mapping: iterate keys(coll)
            self.code.push(Op::LoadLocal(coll_tmp));
            self.code.push(Op::CallEfun("keys".to_owned(), 1));
            self.code.push(Op::StoreLocal(keys_tmp));
            self.code.push(Op::Pop);
            self.code.push(Op::LoadLocal(keys_tmp));
            self.code.push(Op::CallEfun("sizeof".to_owned(), 1));
            self.code.push(Op::StoreLocal(len_tmp));
            self.code.push(Op::Pop);
        } else {
            self.code.push(Op::LoadLocal(coll_tmp));
            self.code.push(Op::CallEfun("sizeof".to_owned(), 1));
            self.code.push(Op::StoreLocal(len_tmp));
            self.code.push(Op::Pop);
        }

        self.code.push(Op::Constant(LpcValue::Int(0)));
        self.code.push(Op::StoreLocal(idx_tmp));
        self.code.push(Op::Pop);

        let loop_start = self.code.len();
        self.breakables.push(Breakable::Loop(LoopContext {
            continue_target: None,
            break_patches: Vec::new(),
            continue_patches: Vec::new(),
        }));
        self.code.push(Op::LoadLocal(idx_tmp));
        self.code.push(Op::LoadLocal(len_tmp));
        self.code.push(Op::Less);
        let end_jump = self.code.len();
        self.code.push(Op::JumpIfFalse(usize::MAX));

        if let Some(keys_tmp) = keys_tmp {
            // k = keys[i]; v = coll[k]
            self.code.push(Op::LoadLocal(keys_tmp));
            self.code.push(Op::LoadLocal(idx_tmp));
            self.code.push(Op::Index);
            self.compile_store_lvalue_name(&variables[0])?;
            self.code.push(Op::Pop);
            self.code.push(Op::LoadLocal(coll_tmp));
            self.load_variable_name(&variables[0])?;
            self.code.push(Op::Index);
            self.compile_store_lvalue_name(&variables[1])?;
            self.code.push(Op::Pop);
        } else {
            self.code.push(Op::LoadLocal(coll_tmp));
            self.code.push(Op::LoadLocal(idx_tmp));
            self.code.push(Op::Index);
            self.compile_store_lvalue_name(&variables[0])?;
            self.code.push(Op::Pop);
        }

        self.compile_statement(body)?;

        let continue_target = self.code.len();
        if let Some(Breakable::Loop(ctx)) = self.breakables.last_mut() {
            ctx.continue_target = Some(continue_target);
            let patches = std::mem::take(&mut ctx.continue_patches);
            for patch in patches {
                self.patch_jump(patch, continue_target);
            }
        }
        self.code.push(Op::LoadLocal(idx_tmp));
        self.code.push(Op::Constant(LpcValue::Int(1)));
        self.code.push(Op::Add);
        self.code.push(Op::StoreLocal(idx_tmp));
        self.code.push(Op::Pop);
        self.code.push(Op::Jump(loop_start));
        let end = self.code.len();
        self.patch_jump(end_jump, end);
        self.finish_loop(end);
        Ok(())
    }

    fn load_variable_name(&mut self, name: &str) -> Result<()> {
        if let Some(index) = self.locals.get(name) {
            self.code.push(Op::LoadLocal(*index));
            return Ok(());
        }
        if let Some(index) = self.globals.iter().position(|candidate| candidate == name) {
            self.code.push(Op::LoadGlobal(index));
            return Ok(());
        }
        // Implicit local for foreach loop variable if not declared.
        let index = self.ensure_local(name);
        self.code.push(Op::LoadLocal(index));
        Ok(())
    }

    fn compile_store_lvalue_name(&mut self, name: &str) -> Result<()> {
        if let Some(index) = self.locals.get(name).copied() {
            self.code.push(Op::StoreLocal(index));
            return Ok(());
        }
        if let Some(index) = self.globals.iter().position(|candidate| candidate == name) {
            self.code.push(Op::StoreGlobal(index));
            return Ok(());
        }
        let index = self.ensure_local(name);
        self.code.push(Op::StoreLocal(index));
        Ok(())
    }

    fn compile_expression(&mut self, expression: &Expr) -> Result<()> {
        let mut compiler = ExpressionCompiler {
            defining_path: self.defining_path,
            globals: self.globals,
            locals: self.locals.clone(),
            callable_names: self.callable_names,
            classes: self.classes,
            code: &mut self.code,
            allow_dollar_args: false,
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
    defining_path: &'a str,
    globals: &'a [String],
    locals: HashMap<String, usize>,
    callable_names: &'a HashSet<String>,
    classes: &'a IndexMap<String, Arc<ClassDef>>,
    code: &'a mut Vec<Op>,
    allow_dollar_args: bool,
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
                self.compile_expression(value)?;
                self.compile_store_lvalue(target)?;
            }
            Expr::Comma { left, right } => {
                self.compile_expression(left)?;
                self.code.push(Op::Pop);
                self.compile_expression(right)?;
            }
            Expr::Unary { operator, operand } => match operator {
                UnaryOp::Negate => {
                    self.compile_expression(operand)?;
                    self.code.push(Op::Negate);
                }
                UnaryOp::Not => {
                    self.compile_expression(operand)?;
                    self.code.push(Op::Not);
                }
                UnaryOp::BitNot => {
                    self.compile_expression(operand)?;
                    self.code.push(Op::BitNot);
                }
                UnaryOp::Deref => {
                    self.compile_expression(operand)?;
                }
                UnaryOp::PreIncrement => {
                    self.compile_pre_increment(operand, 1)?;
                }
                UnaryOp::PreDecrement => {
                    self.compile_pre_increment(operand, -1)?;
                }
            },
            Expr::Postfix { operator, operand } => {
                let delta = match operator {
                    PostfixOp::Increment => 1,
                    PostfixOp::Decrement => -1,
                };
                self.compile_post_increment(operand, delta)?;
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                match operator {
                    BinaryOp::And => {
                        // LPC short-circuit: if left is falsy, yield left and skip right.
                        self.compile_expression(left)?;
                        self.code.push(Op::Dup);
                        let jump = self.code.len();
                        self.code.push(Op::JumpIfFalse(usize::MAX));
                        self.code.push(Op::Pop);
                        self.compile_expression(right)?;
                        let end = self.code.len();
                        patch_expression_jump(self.code, jump, end);
                    }
                    BinaryOp::Or => {
                        // LPC short-circuit: if left is truthy, yield left and skip right.
                        self.compile_expression(left)?;
                        self.code.push(Op::Dup);
                        self.code.push(Op::Not);
                        let jump = self.code.len();
                        self.code.push(Op::JumpIfFalse(usize::MAX));
                        self.code.push(Op::Pop);
                        self.compile_expression(right)?;
                        let end = self.code.len();
                        patch_expression_jump(self.code, jump, end);
                    }
                    _ => {
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
                            BinaryOp::BitAnd => Op::BitAnd,
                            BinaryOp::BitOr => Op::BitOr,
                            BinaryOp::BitXor => Op::BitXor,
                            BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
                        });
                    }
                }
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
                } else if name == "sscanf" && arguments.len() > 2 {
                    self.compile_sscanf(arguments)?;
                } else if name == "parse_command" && arguments.len() > 3 {
                    self.compile_parse_command(arguments)?;
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
            Expr::InheritCall {
                inherit,
                name,
                arguments,
            } => {
                for argument in arguments {
                    self.compile_expression(argument)?;
                }
                self.code.push(Op::CallInherit(
                    inherit.clone(),
                    name.clone(),
                    arguments.len(),
                ));
            }
            Expr::CallValue { callee, arguments } => {
                for argument in arguments {
                    self.compile_expression(argument)?;
                }
                self.compile_expression(callee)?;
                self.code.push(Op::CallValue(arguments.len()));
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
            Expr::FunctionalNamed { name, bound } => {
                for argument in bound {
                    self.compile_expression(argument)?;
                }
                self.code
                    .push(Op::MakeNamedFunction(name.clone(), bound.len()));
            }
            Expr::FunctionalExpr { body } => {
                let max_arg = max_dollar_arg(body);
                let mut code = Vec::new();
                {
                    let mut nested = ExpressionCompiler {
                        defining_path: self.defining_path,
                        globals: self.globals,
                        locals: HashMap::new(),
                        callable_names: self.callable_names,
                        classes: self.classes,
                        code: &mut code,
                        allow_dollar_args: true,
                    };
                    nested.compile_expression(body)?;
                }
                code.push(Op::Return);
                let function = Arc::new(FunctionInfo {
                    name: "<functional>".to_owned(),
                    parameters: (1..=max_arg).map(|n| format!("${n}")).collect(),
                    local_count: max_arg,
                    code,
                    source_line: 0,
                    defining_path: self.defining_path.to_owned(),
                });
                self.code.push(Op::MakeExprFunction(function));
            }
            Expr::DollarArg(index) => {
                if !self.allow_dollar_args {
                    bail!("$N placeholders are only valid inside (: … :) functionals");
                }
                self.code.push(Op::LoadLocal(index - 1));
            }
            Expr::Cast { type_name, value } => {
                self.compile_expression(value)?;
                self.code.push(Op::Cast(type_name.clone()));
            }
            Expr::Catch(inner) => {
                let enter = self.code.len();
                self.code.push(Op::EnterCatch(usize::MAX));
                self.compile_expression(inner)?;
                self.code.push(Op::LeaveCatchSuccess);
                let end_jump = self.code.len();
                self.code.push(Op::Jump(usize::MAX));
                let handler = self.code.len();
                if let Op::EnterCatch(target) = &mut self.code[enter] {
                    *target = handler;
                }
                // Error string is pushed by the interpreter when jumping here.
                let end = self.code.len();
                patch_expression_jump(self.code, end_jump, end);
            }
            Expr::Member { object, field } => {
                self.compile_expression(object)?;
                self.code.push(Op::MemberGet(field.clone()));
            }
            Expr::NewClass { class_name } => {
                let Some(def) = self.classes.get(class_name) else {
                    bail!("unknown class {class_name}");
                };
                self.code.push(Op::NewClass(def.clone()));
            }
        }
        Ok(())
    }

    /// MudOS `sscanf(str, fmt, out...)` — parse captures then store into out lvalues.
    fn compile_sscanf(&mut self, arguments: &[Expr]) -> Result<()> {
        if arguments.len() < 2 {
            bail!("sscanf requires at least two arguments");
        }
        self.compile_expression(&arguments[0])?;
        self.compile_expression(&arguments[1])?;
        // Returns array of captures.
        self.code
            .push(Op::CallEfun("sscanf_values".to_owned(), 2));
        let outs = &arguments[2..];
        for (index, target) in outs.iter().enumerate() {
            if !is_lvalue(target) {
                bail!("sscanf output argument must be an lvalue");
            }
            // MudOS: unmatched captures leave the out parameter unchanged.
            // Indexing past sizeof(captures) used to store 0/null, so
            // `substr()` / `query_title()` concatenated a trailing "0".
            self.code.push(Op::Dup);
            self.code.push(Op::CallEfun("sizeof".to_owned(), 1));
            self.code
                .push(Op::Constant(LpcValue::Int((index + 1) as i64)));
            self.code.push(Op::Swap);
            self.code.push(Op::Less);
            let skip = self.code.len();
            self.code.push(Op::JumpIfFalse(usize::MAX));
            self.code.push(Op::Dup);
            self.code
                .push(Op::Constant(LpcValue::Int((index + 1) as i64)));
            self.code.push(Op::Index);
            self.compile_store_lvalue(target)?;
            self.code.push(Op::Pop);
            let after = self.code.len();
            patch_expression_jump(self.code, skip, after);
        }
        // MudOS return value counts `%*` fields too (monster receive_message).
        self.code.push(Op::Constant(LpcValue::Int(0)));
        self.code.push(Op::Index);
        Ok(())
    }

    /// MudOS `parse_command(cmd, env, pattern, out...)` — match then store captures.
    /// `parse_command_values` returns `({ matched, cap... })`; leftover stack value is 0/1.
    fn compile_parse_command(&mut self, arguments: &[Expr]) -> Result<()> {
        if arguments.len() < 3 {
            bail!("parse_command requires command, env, and pattern");
        }
        self.compile_expression(&arguments[0])?;
        self.compile_expression(&arguments[1])?;
        self.compile_expression(&arguments[2])?;
        self.code
            .push(Op::CallEfun("parse_command_values".to_owned(), 3));
        let outs = &arguments[3..];
        for (index, target) in outs.iter().enumerate() {
            if !is_lvalue(target) {
                bail!("parse_command output argument must be an lvalue");
            }
            self.code.push(Op::Dup);
            self.code
                .push(Op::Constant(LpcValue::Int((index + 1) as i64)));
            self.code.push(Op::Index);
            self.compile_store_lvalue(target)?;
            self.code.push(Op::Pop);
        }
        self.code.push(Op::Constant(LpcValue::Int(0)));
        self.code.push(Op::Index);
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

    fn compile_pre_increment(&mut self, operand: &Expr, delta: i64) -> Result<()> {
        match operand {
            Expr::Variable(name) => {
                self.load_variable(name)?;
                self.code.push(Op::Constant(LpcValue::Int(delta)));
                self.code.push(Op::Add);
                self.store_variable(name)?;
            }
            Expr::Index { .. } | Expr::Member { .. } => {
                self.compile_expression(operand)?;
                self.code.push(Op::Constant(LpcValue::Int(delta)));
                self.code.push(Op::Add);
                self.compile_store_lvalue(operand)?;
            }
            _ => bail!("++/-- requires a variable, index, or member"),
        }
        Ok(())
    }

    fn compile_post_increment(&mut self, operand: &Expr, delta: i64) -> Result<()> {
        match operand {
            Expr::Variable(name) => {
                self.load_variable(name)?;
                self.code.push(Op::Dup);
                self.code.push(Op::Constant(LpcValue::Int(delta)));
                self.code.push(Op::Add);
                self.store_variable(name)?;
                self.code.push(Op::Pop);
            }
            Expr::Index { .. } | Expr::Member { .. } => {
                self.compile_expression(operand)?;
                self.code.push(Op::Dup);
                self.code.push(Op::Constant(LpcValue::Int(delta)));
                self.code.push(Op::Add);
                self.compile_store_lvalue(operand)?;
                self.code.push(Op::Pop);
            }
            _ => bail!("++/-- requires a variable, index, or member"),
        }
        Ok(())
    }

    /// Store stack-top into `target`, leaving the stored value on the stack.
    fn compile_store_lvalue(&mut self, target: &Expr) -> Result<()> {
        match target {
            Expr::Variable(name) => self.store_variable(name),
            Expr::Index { value: base, index } => {
                // stack: value
                // MudOS: `m[k] = v` as an expression yields `v`, not the mapping.
                // more() does `sizeof(__More["lines"] = what)` and needs the array size.
                self.code.push(Op::Dup); // value, value
                self.compile_expression(base)?; // value, value, base
                self.code.push(Op::Swap); // value, base, value
                self.compile_expression(index)?; // value, base, value, index
                self.code.push(Op::Swap); // value, base, index, value
                self.code.push(Op::IndexSet); // value, updated_base
                self.compile_store_lvalue(base)?; // value, updated_base
                self.code.push(Op::Pop); // value
                Ok(())
            }
            Expr::Slice {
                value: base,
                start,
                end,
            } => {
                // stack: replacement
                self.code.push(Op::Dup); // repl, repl
                self.compile_expression(base)?; // repl, repl, base
                self.code.push(Op::Swap); // repl, base, repl
                if let Some(start) = start {
                    self.compile_expression(start)?;
                } else {
                    self.code.push(Op::Constant(LpcValue::Null));
                } // repl, base, repl, start
                self.code.push(Op::Swap); // repl, base, start, repl
                if let Some(end) = end {
                    self.compile_expression(end)?;
                } else {
                    self.code.push(Op::Constant(LpcValue::Null));
                } // repl, base, start, repl, end
                self.code.push(Op::Swap); // repl, base, start, end, repl
                self.code.push(Op::SliceSet); // repl, updated_base
                self.compile_store_lvalue(base)?;
                self.code.push(Op::Pop); // repl
                Ok(())
            }
            Expr::Member { object, field } => {
                // stack: value
                self.compile_expression(object)?; // value, object
                self.code.push(Op::Swap); // object, value
                self.code.push(Op::MemberSet(field.clone())); // value
                Ok(())
            }
            _ => bail!("assignment targets must be variables, index, slice, or member expressions (got {target:?})"),
        }
    }
}

fn patch_expression_jump(code: &mut [Op], offset: usize, target: usize) {
    match &mut code[offset] {
        Op::Jump(value) | Op::JumpIfFalse(value) => *value = target,
        _ => unreachable!("attempted to patch a non-jump operation"),
    }
}

fn max_dollar_arg(expression: &Expr) -> usize {
    match expression {
        Expr::Null
        | Expr::Int(_)
        | Expr::Float(_)
        | Expr::String(_)
        | Expr::Variable(_) => 0,
        Expr::DollarArg(index) => *index,
        Expr::Array(values) => values.iter().map(max_dollar_arg).max().unwrap_or(0),
        Expr::Mapping(entries) => entries
            .iter()
            .map(|(key, value)| max_dollar_arg(key).max(max_dollar_arg(value)))
            .max()
            .unwrap_or(0),
        Expr::Assign { target, value } => max_dollar_arg(target).max(max_dollar_arg(value)),
        Expr::Comma { left, right } => max_dollar_arg(left).max(max_dollar_arg(right)),
        Expr::Unary { operand, .. }
        | Expr::Cast { value: operand, .. }
        | Expr::Postfix { operand, .. } => max_dollar_arg(operand),
        Expr::Binary { left, right, .. } => max_dollar_arg(left).max(max_dollar_arg(right)),
        Expr::Conditional {
            condition,
            then_value,
            else_value,
        } => max_dollar_arg(condition)
            .max(max_dollar_arg(then_value))
            .max(max_dollar_arg(else_value)),
        Expr::Call { arguments, .. }
        | Expr::FunctionalNamed { bound: arguments, .. }
        | Expr::InheritCall { arguments, .. } => {
            arguments.iter().map(max_dollar_arg).max().unwrap_or(0)
        }
        Expr::CallValue { callee, arguments } => arguments
            .iter()
            .map(max_dollar_arg)
            .max()
            .unwrap_or(0)
            .max(max_dollar_arg(callee)),
        Expr::Index { value, index } => max_dollar_arg(value).max(max_dollar_arg(index)),
        Expr::Slice { value, start, end } => {
            let mut max = max_dollar_arg(value);
            if let Some(start) = start {
                max = max.max(max_dollar_arg(start));
            }
            if let Some(end) = end {
                max = max.max(max_dollar_arg(end));
            }
            max
        }
        Expr::FunctionalExpr { body } => max_dollar_arg(body),
        Expr::Catch(inner) => max_dollar_arg(inner),
        Expr::Member { object, .. } => max_dollar_arg(object),
        Expr::NewClass { .. } => 0,
    }
}
