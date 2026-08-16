use super::object::ObjectRef;
use super::program::{FunctionInfo, Op, Program};
use super::value::LpcValue;
use super::MudWorld;
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use std::sync::Arc;

pub struct Interpreter<'a> {
    pub world: &'a MudWorld,
    pub current_object: ObjectRef,
    pub this_player: Option<ObjectRef>,
    pub previous_object: Option<ObjectRef>,
    cost: usize,
    max_cost: usize,
    call_depth: usize,
}

impl<'a> Interpreter<'a> {
    pub fn new(
        world: &'a MudWorld,
        current_object: ObjectRef,
        this_player: Option<ObjectRef>,
        previous_object: Option<ObjectRef>,
        max_cost: usize,
    ) -> Self {
        Self {
            world,
            current_object,
            this_player,
            previous_object,
            cost: 0,
            max_cost,
            call_depth: 0,
        }
    }

    pub fn reset_cost(&mut self) {
        self.cost = 0;
    }

    pub fn find_function(program: &Arc<Program>, name: &str) -> Option<FunctionInfo> {
        if let Some(function) = program.local_functions.get(name) {
            return Some(function.clone());
        }
        for inherited in &program.inherit_programs {
            if Self::find_function(inherited, name).is_some() {
                return program
                    .functions
                    .get(name)
                    .cloned()
                    .or_else(|| Self::find_function(inherited, name));
            }
        }
        None
    }

    pub fn find_inherited_function(
        program: &Arc<Program>,
        inherit: Option<&str>,
        name: &str,
    ) -> Option<FunctionInfo> {
        for inherited in &program.inherit_programs {
            if let Some(label) = inherit {
                let basename = inherited
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(inherited.path.as_str());
                if basename != label && inherited.path != label {
                    // Keep searching other inherits.
                    if let Some(function) =
                        Self::find_inherited_function(inherited, inherit, name)
                    {
                        return Some(function);
                    }
                    continue;
                }
            }
            // Prefer the function body defined on this inherit (or deeper), not a
            // later override merged into a child — use local_functions first via
            // find_function which already walks inherit_programs.
            if let Some(function) = Self::find_function(inherited, name) {
                return Some(function);
            }
        }
        None
    }

    /// Locate `path` within `root` or any nested inherit program.
    pub fn find_program_by_path(root: &Arc<Program>, path: &str) -> Option<Arc<Program>> {
        if root.path == path {
            return Some(root.clone());
        }
        for inherited in &root.inherit_programs {
            if let Some(found) = Self::find_program_by_path(inherited, path) {
                return Some(found);
            }
        }
        None
    }

    pub fn apply(&mut self, name: &str, arguments: Vec<LpcValue>) -> Result<LpcValue> {
        let program = self.current_object.lock().program.clone();
        let function = Self::find_function(&program, name)
            .with_context(|| format!("{} has no function {name}", program.path))?;
        self.execute(function, arguments)
    }

    pub fn call_function(
        &mut self,
        object: ObjectRef,
        name: &str,
        arguments: Vec<LpcValue>,
    ) -> Result<LpcValue> {
        let program = object.lock().program.clone();
        let Some(function) = Self::find_function(&program, name) else {
            // MudOS: calling a missing function via call_other / -> returns 0.
            return Ok(LpcValue::Null);
        };
        let old_current = std::mem::replace(&mut self.current_object, object);
        let old_previous =
            std::mem::replace(&mut self.previous_object, Some(old_current.clone()));
        let result = self.execute(function, arguments);
        self.current_object = old_current;
        self.previous_object = old_previous;
        result
    }

    fn execute(
        &mut self,
        function: FunctionInfo,
        arguments: Vec<LpcValue>,
    ) -> Result<LpcValue> {
        // Debug builds use large frames per execute_inner; keep this low enough
        // to fail with a useful error before Windows STATUS_STACK_OVERFLOW.
        const MAX_CALL_DEPTH: usize = 96;
        if self.call_depth >= MAX_CALL_DEPTH {
            bail!(
                "maximum call depth exceeded at {}::{}",
                self.current_object.lock().name,
                function.name
            );
        }
        self.call_depth += 1;
        let result = self.execute_inner(&function, arguments);
        self.call_depth -= 1;
        result.with_context(|| format!("in function {}", function.name))
    }

    fn execute_inner(
        &mut self,
        function: &FunctionInfo,
        arguments: Vec<LpcValue>,
    ) -> Result<LpcValue> {
        // MudOS pads omitted arguments with 0/null (especially varargs like heart_beat).
        let mut arguments = arguments;
        while arguments.len() < function.arity() {
            arguments.push(LpcValue::Null);
        }
        let mut locals = vec![LpcValue::Null; function.local_count.max(arguments.len())];
        for (slot, value) in locals.iter_mut().zip(arguments) {
            *slot = value;
        }
        let mut stack = Vec::new();
        let mut instruction = 0;
        let mut catch_frames: Vec<(usize, usize)> = Vec::new();
        while instruction < function.code.len() {
            self.cost += 1;
            if self.cost > self.max_cost {
                bail!("maximum evaluation cost of {} exceeded", self.max_cost);
            }
            let operation = function.code[instruction].clone();
            instruction += 1;
            if matches!(operation, Op::Return) {
                return Ok(stack.pop().unwrap_or(LpcValue::Null));
            }
            let step: Result<()> = (|| {
                match operation {
                Op::Constant(value) => stack.push(value),
                Op::LoadGlobal(index) => {
                    let value = self
                        .current_object
                        .lock()
                        .globals
                        .get(index)
                        .cloned()
                        .with_context(|| format!("invalid global slot {index}"))?;
                    stack.push(value);
                }
                Op::StoreGlobal(index) => {
                    let value = pop(&mut stack)?;
                    let mut object = self.current_object.lock();
                    let slot = object
                        .globals
                        .get_mut(index)
                        .with_context(|| format!("invalid global slot {index}"))?;
                    *slot = value.clone();
                    drop(object);
                    stack.push(value);
                }
                Op::LoadLocal(index) => {
                    stack.push(
                        locals
                            .get(index)
                            .cloned()
                            .with_context(|| format!("invalid local slot {index}"))?,
                    );
                }
                Op::StoreLocal(index) => {
                    let value = pop(&mut stack)?;
                    let slot = locals
                        .get_mut(index)
                        .with_context(|| format!("invalid local slot {index}"))?;
                    *slot = value.clone();
                    stack.push(value);
                }
                Op::Pop => {
                    pop(&mut stack)?;
                }
                Op::Dup => {
                    let value = stack.last().cloned().context("stack underflow")?;
                    stack.push(value);
                }
                Op::Swap => {
                    let right = pop(&mut stack)?;
                    let left = pop(&mut stack)?;
                    stack.push(right);
                    stack.push(left);
                }
                Op::Add => binary(&mut stack, add)?,
                Op::Subtract => binary(&mut stack, subtract)?,
                Op::Multiply => binary(&mut stack, |left, right| {
                    numeric(left, right, |a, b| a * b, |a, b| a * b)
                })?,
                Op::Divide => binary(&mut stack, divide)?,
                Op::Modulo => binary(&mut stack, |left, right| {
                    let divisor = right.as_int().context("modulo divisor must be an integer")?;
                    if divisor == 0 {
                        return Ok(LpcValue::Int(0));
                    }
                    Ok(LpcValue::Int(
                        left.as_int().context("modulo value must be an integer")? % divisor,
                    ))
                })?,
                Op::Negate => {
                    let value = pop(&mut stack)?;
                    stack.push(match value {
                        LpcValue::Float(value) => LpcValue::Float(-value),
                        value => LpcValue::Int(
                            -value.as_int().context("negation requires a number")?,
                        ),
                    });
                }
                Op::Not => {
                    let value = pop(&mut stack)?;
                    stack.push(boolean(!value.is_truthy()));
                }
                Op::Equal => binary(&mut stack, |left, right| Ok(boolean(left == right)))?,
                Op::NotEqual => {
                    binary(&mut stack, |left, right| Ok(boolean(left != right)))?
                }
                Op::Less => comparison(&mut stack, |ordering| ordering.is_lt())?,
                Op::LessEqual => comparison(&mut stack, |ordering| ordering.is_le())?,
                Op::Greater => comparison(&mut stack, |ordering| ordering.is_gt())?,
                Op::GreaterEqual => comparison(&mut stack, |ordering| ordering.is_ge())?,
                Op::And => binary(&mut stack, |left, right| {
                    Ok(boolean(left.is_truthy() && right.is_truthy()))
                })?,
                Op::Or => binary(&mut stack, |left, right| {
                    Ok(boolean(left.is_truthy() || right.is_truthy()))
                })?,
                Op::BitAnd => binary(&mut stack, bit_and)?,
                Op::BitOr => binary(&mut stack, bit_or)?,
                Op::BitXor => binary(&mut stack, |left, right| {
                    Ok(LpcValue::Int(
                        left.as_int().unwrap_or(0) ^ right.as_int().unwrap_or(0),
                    ))
                })?,
                Op::BitNot => {
                    let value = pop(&mut stack)?;
                    stack.push(LpcValue::Int(!value.as_int().unwrap_or(0)));
                }
                Op::Index => {
                    let index = pop(&mut stack)?;
                    let value = pop(&mut stack)?;
                    stack.push(index_value(value, index)?);
                }
                Op::IndexSet => {
                    let value = pop(&mut stack)?;
                    let index = pop(&mut stack)?;
                    let container = pop(&mut stack)?;
                    stack.push(index_set_value(container, index, value)?);
                }
                Op::Slice => {
                    let end = pop(&mut stack)?;
                    let start = pop(&mut stack)?;
                    let value = pop(&mut stack)?;
                    stack.push(slice_value(value, start, end)?);
                }
                Op::SliceSet => {
                    let value = pop(&mut stack)?;
                    let end = pop(&mut stack)?;
                    let start = pop(&mut stack)?;
                    let container = pop(&mut stack)?;
                    stack.push(slice_set_value(container, start, end, value)?);
                }
                Op::MakeArray(count) => {
                    if stack.len() < count {
                        bail!("stack underflow while constructing array");
                    }
                    let values = stack.split_off(stack.len() - count);
                    stack.push(LpcValue::Array(values));
                }
                Op::MakeMapping(count) => {
                    if stack.len() < count * 2 {
                        bail!("stack underflow while constructing mapping");
                    }
                    let values = stack.split_off(stack.len() - count * 2);
                    let mut mapping = IndexMap::new();
                    for pair in values.chunks_exact(2) {
                        let key = mapping_key(&pair[0]);
                        mapping.insert(key, pair[1].clone());
                    }
                    stack.push(LpcValue::Mapping(mapping));
                }
                Op::Jump(target) => instruction = checked_target(target, &function.code)?,
                Op::JumpIfFalse(target) => {
                    if !pop(&mut stack)?.is_truthy() {
                        instruction = checked_target(target, &function.code)?;
                    }
                }
                Op::Call(name, count) => {
                    let arguments = pop_arguments(&mut stack, count)?;
                    let program = self.current_object.lock().program.clone();
                    let called = Self::find_function(&program, &name)
                        .with_context(|| format!("{} has no function {name}", program.path))?;
                    let result = self.execute(called, arguments)?;
                    stack.push(result);
                }
                Op::CallInherit(inherit, name, count) => {
                    let arguments = pop_arguments(&mut stack, count)?;
                    // MudOS `efun::foo()` invokes the real efun, bypassing simul_efun.
                    if inherit.as_deref() == Some("efun") {
                        if let Some(efun) = self.world.efuns.get(name.as_str()) {
                            stack.push(efun(self, arguments)?);
                        } else {
                            bail!("unknown efun {name}");
                        }
                    } else {
                        let object_program = self.current_object.lock().program.clone();
                        // Bare `::foo` must resolve from the *defining* file's inherits
                        // (MudOS), not the leaf object's merged function table.
                        let search_root = Self::find_program_by_path(
                            &object_program,
                            &function.defining_path,
                        )
                        .unwrap_or_else(|| object_program.clone());
                        let called = Self::find_inherited_function(
                            &search_root,
                            inherit.as_deref(),
                            &name,
                        )
                        .with_context(|| {
                            format!(
                                "{} has no inherited function {}{name} (from {})",
                                search_root.path,
                                inherit
                                    .as_ref()
                                    .map(|value| format!("{value}::"))
                                    .unwrap_or_default(),
                                function.defining_path
                            )
                        })?;
                        let result = self.execute(called, arguments)?;
                        stack.push(result);
                    }
                }
                Op::CallValue(count) => {
                    let callee = pop(&mut stack)?;
                    let arguments = pop_arguments(&mut stack, count)?;
                    let result = match callee {
                        LpcValue::Function(function) => {
                            self.call_lpc_function(&function, arguments)?
                        }
                        LpcValue::String(name) => {
                            if let Some(efun) = self.world.efuns.get(name.as_str()) {
                                efun(self, arguments)?
                            } else {
                                let program = self.current_object.lock().program.clone();
                                let called = Self::find_function(&program, &name)
                                    .with_context(|| format!("unknown function {name}"))?;
                                self.execute(called, arguments)?
                            }
                        }
                        other => bail!("cannot call value of type {}", other.type_name()),
                    };
                    stack.push(result);
                }
                Op::CallEfun(name, count) => {
                    let arguments = pop_arguments(&mut stack, count)?;
                    if let Some(efun) = self.world.efuns.get(&name) {
                        stack.push(efun(self, arguments)?);
                    } else if let Some(simul) = self.world.simul_efun() {
                        let result = self.call_function(simul, &name, arguments)?;
                        stack.push(result);
                    } else {
                        bail!("unknown efun {name}");
                    }
                }
                Op::ThisObject => stack.push(LpcValue::Object(self.current_object.clone())),
                Op::MakeNamedFunction(name, count) => {
                    let bound = pop_arguments(&mut stack, count)?;
                    stack.push(LpcValue::Function(Arc::new(
                        crate::vm::value::LpcFunction {
                            owner: self.current_object.clone(),
                            kind: crate::vm::value::FunctionKind::Named { name, bound },
                        },
                    )));
                }
                Op::MakeExprFunction(function) => {
                    stack.push(LpcValue::Function(Arc::new(
                        crate::vm::value::LpcFunction {
                            owner: self.current_object.clone(),
                            kind: crate::vm::value::FunctionKind::Expression { function },
                        },
                    )));
                }
                Op::Cast(type_name) => {
                    let value = pop(&mut stack)?;
                    stack.push(cast_value(value, &type_name)?);
                }
                Op::EnterCatch(handler) => {
                    catch_frames.push((handler, stack.len()));
                }
                Op::LeaveCatchSuccess => {
                    catch_frames.pop();
                    pop(&mut stack)?;
                    stack.push(LpcValue::Int(0));
                }
                Op::NewClass(def) => {
                    stack.push(LpcValue::Class(crate::vm::value::ClassInstance::new(def)));
                }
                Op::MemberGet(field) => {
                    let value = pop(&mut stack)?;
                    match value {
                        LpcValue::Class(instance) => {
                            let index = instance
                                .def
                                .fields
                                .iter()
                                .position(|name| name == &field)
                                .with_context(|| format!("class has no field {field}"))?;
                            let fields = instance.fields.lock();
                            stack.push(fields.get(index).cloned().unwrap_or(LpcValue::Null));
                        }
                        LpcValue::Null => stack.push(LpcValue::Null),
                        other => bail!(
                            "member access requires a class (got {})",
                            other.type_name()
                        ),
                    }
                }
                Op::MemberSet(field) => {
                    let value = pop(&mut stack)?;
                    let instance = pop(&mut stack)?;
                    match instance {
                        LpcValue::Class(instance) => {
                            let index = instance
                                .def
                                .fields
                                .iter()
                                .position(|name| name == &field)
                                .with_context(|| format!("class has no field {field}"))?;
                            instance.fields.lock()[index] = value.clone();
                            stack.push(value);
                        }
                        other => bail!(
                            "member assignment requires a class (got {})",
                            other.type_name()
                        ),
                    }
                }
                Op::Return => unreachable!("handled above"),
                }
                Ok(())
            })();
            if let Err(err) = step {
                if let Some((handler, stack_len)) = catch_frames.pop() {
                    stack.truncate(stack_len);
                    stack.push(LpcValue::String(format!("{err:#}")));
                    instruction = checked_target(handler, &function.code)?;
                } else {
                    return Err(err);
                }
            }
        }
        Ok(LpcValue::Null)
    }

    pub fn call_lpc_function(
        &mut self,
        function: &crate::vm::value::LpcFunction,
        mut arguments: Vec<LpcValue>,
    ) -> Result<LpcValue> {
        match &function.kind {
            crate::vm::value::FunctionKind::Named { name, bound } => {
                let mut call_args = bound.clone();
                call_args.append(&mut arguments);
                if Self::find_function(&function.owner.lock().program, name).is_some() {
                    return self.call_function(function.owner.clone(), name, call_args);
                }
                if let Some(efun) = self.world.efuns.get(name) {
                    return efun(self, call_args);
                }
                bail!("unknown function {name} in functional");
            }
            crate::vm::value::FunctionKind::Expression { function: body } => {
                let old_current =
                    std::mem::replace(&mut self.current_object, function.owner.clone());
                let result = self.execute(body.as_ref().clone(), arguments);
                self.current_object = old_current;
                result
            }
        }
    }
}

fn pop(stack: &mut Vec<LpcValue>) -> Result<LpcValue> {
    stack.pop().context("stack underflow")
}

fn cast_value(value: LpcValue, type_name: &str) -> Result<LpcValue> {
    Ok(match type_name {
        "int" => LpcValue::Int(value.as_int().unwrap_or(0)),
        "float" => match value {
            LpcValue::Float(v) => LpcValue::Float(v),
            LpcValue::Int(v) => LpcValue::Float(v as f64),
            LpcValue::String(s) => LpcValue::Float(s.trim().parse().unwrap_or(0.0)),
            _ => LpcValue::Float(0.0),
        },
        // `(string *)arr` is a no-op pointer cast in MudOS; parser strips `*`.
        "string" => match value {
            LpcValue::Array(array) => LpcValue::Array(array),
            other => LpcValue::String(other.to_string()),
        },
        "object" => match value {
            LpcValue::Object(object) => LpcValue::Object(object),
            LpcValue::Array(array) => LpcValue::Array(array),
            _ => LpcValue::Null,
        },
        "mapping" => match value {
            LpcValue::Mapping(mapping) => LpcValue::Mapping(mapping),
            _ => LpcValue::Mapping(IndexMap::new()),
        },
        "mixed" | "void" | "function" => value,
        other if other.starts_with("class:") => match value {
            LpcValue::Class(_) | LpcValue::Null => value,
            _ => LpcValue::Null,
        },
        other => bail!("unsupported cast to {other}"),
    })
}

fn pop_arguments(stack: &mut Vec<LpcValue>, count: usize) -> Result<Vec<LpcValue>> {
    if stack.len() < count {
        bail!("stack underflow while collecting call arguments");
    }
    Ok(stack.split_off(stack.len() - count))
}

fn binary(
    stack: &mut Vec<LpcValue>,
    operation: impl FnOnce(LpcValue, LpcValue) -> Result<LpcValue>,
) -> Result<()> {
    let right = pop(stack)?;
    let left = pop(stack)?;
    stack.push(operation(left, right)?);
    Ok(())
}

fn add(left: LpcValue, right: LpcValue) -> Result<LpcValue> {
    match (left, right) {
        (LpcValue::String(left), right) => Ok(LpcValue::String(left + &right.to_string())),
        (left, LpcValue::String(right)) => Ok(LpcValue::String(left.to_string() + &right)),
        (LpcValue::Array(mut left), LpcValue::Array(right)) => {
            left.extend(right);
            Ok(LpcValue::Array(left))
        }
        (LpcValue::Null, LpcValue::Array(right)) | (LpcValue::Array(right), LpcValue::Null) => {
            Ok(LpcValue::Array(right))
        }
        (LpcValue::Mapping(mut left), LpcValue::Mapping(right)) => {
            for (key, value) in right {
                left.insert(key, value);
            }
            Ok(LpcValue::Mapping(left))
        }
        (LpcValue::Null, LpcValue::Mapping(right)) | (LpcValue::Mapping(right), LpcValue::Null) => {
            Ok(LpcValue::Mapping(right))
        }
        (LpcValue::Null, LpcValue::Null) => Ok(LpcValue::Int(0)),
        (LpcValue::Null, other) | (other, LpcValue::Null) => match other {
            LpcValue::Int(v) => Ok(LpcValue::Int(v)),
            LpcValue::Float(v) => Ok(LpcValue::Float(v)),
            LpcValue::String(s) => Ok(LpcValue::String(s)),
            LpcValue::Array(a) => Ok(LpcValue::Array(a)),
            LpcValue::Mapping(m) => Ok(LpcValue::Mapping(m)),
            other => numeric(LpcValue::Null, other, |a, b| a + b, |a, b| a + b),
        },
        (left, right) => numeric(left, right, |a, b| a + b, |a, b| a + b),
    }
}

fn subtract(left: LpcValue, right: LpcValue) -> Result<LpcValue> {
    match (left, right) {
        (LpcValue::Array(left), LpcValue::Array(right)) => Ok(LpcValue::Array(
            left.into_iter()
                .filter(|value| !right.contains(value))
                .collect(),
        )),
        (left, right) => numeric(left, right, |a, b| a - b, |a, b| a - b),
    }
}

fn bit_and(left: LpcValue, right: LpcValue) -> Result<LpcValue> {
    match (left, right) {
        (LpcValue::Array(left), LpcValue::Array(right)) => Ok(LpcValue::Array(
            left.into_iter()
                .filter(|value| right.contains(value))
                .collect(),
        )),
        (left, right) => Ok(LpcValue::Int(
            left.as_int().unwrap_or(0) & right.as_int().unwrap_or(0),
        )),
    }
}

fn bit_or(left: LpcValue, right: LpcValue) -> Result<LpcValue> {
    match (left, right) {
        (LpcValue::Array(mut left), LpcValue::Array(right)) => {
            for value in right {
                if !left.contains(&value) {
                    left.push(value);
                }
            }
            Ok(LpcValue::Array(left))
        }
        (left, right) => Ok(LpcValue::Int(
            left.as_int().unwrap_or(0) | right.as_int().unwrap_or(0),
        )),
    }
}

fn divide(left: LpcValue, right: LpcValue) -> Result<LpcValue> {
    if matches!(&right, LpcValue::Int(0))
        || matches!(&right, LpcValue::Float(value) if *value == 0.0)
    {
        // Soft like MudOS runtime: 0 rather than aborting the command.
        return Ok(LpcValue::Int(0));
    }
    numeric(left, right, |a, b| a / b, |a, b| a / b)
}

fn numeric(
    left: LpcValue,
    right: LpcValue,
    integer: impl FnOnce(i64, i64) -> i64,
    float: impl FnOnce(f64, f64) -> f64,
) -> Result<LpcValue> {
    match (left, right) {
        (LpcValue::Int(left), LpcValue::Int(right)) => {
            Ok(LpcValue::Int(integer(left, right)))
        }
        (left, right) => {
            let left = number(&left)?;
            let right = number(&right)?;
            Ok(LpcValue::Float(float(left, right)))
        }
    }
}

fn number(value: &LpcValue) -> Result<f64> {
    match value {
        LpcValue::Int(value) => Ok(*value as f64),
        LpcValue::Float(value) => Ok(*value),
        // MudOS mudlibs often compare unset properties (0/null) numerically.
        LpcValue::Null => Ok(0.0),
        LpcValue::String(s) => Ok(s.trim().parse::<f64>().unwrap_or(0.0)),
        LpcValue::Mapping(_) | LpcValue::Array(_) => Ok(0.0),
        _ => bail!("{} is not numeric", value.type_name()),
    }
}

fn comparison(
    stack: &mut Vec<LpcValue>,
    predicate: impl FnOnce(std::cmp::Ordering) -> bool,
) -> Result<()> {
    let right = pop(stack)?;
    let left = pop(stack)?;
    let ordering = match (&left, &right) {
        (LpcValue::String(left), LpcValue::String(right)) => left.cmp(right),
        _ => number(&left)?
            .partial_cmp(&number(&right)?)
            .context("cannot compare NaN")?,
    };
    stack.push(boolean(predicate(ordering)));
    Ok(())
}

fn index_value(value: LpcValue, index: LpcValue) -> Result<LpcValue> {
    match value {
        LpcValue::Array(values) => {
            let index = normalized_index(index, values.len())?;
            Ok(values.get(index).cloned().unwrap_or(LpcValue::Null))
        }
        LpcValue::String(value) => {
            let characters: Vec<char> = value.chars().collect();
            let index = normalized_index(index, characters.len())?;
            Ok(characters
                .get(index)
                .map(|character| LpcValue::Int(*character as i64))
                .unwrap_or(LpcValue::Null))
        }
        LpcValue::Mapping(values) => Ok(values
            .get(&mapping_key(&index))
            .cloned()
            .unwrap_or(LpcValue::Null)),
        LpcValue::Null => Ok(LpcValue::Null),
        LpcValue::Int(_) | LpcValue::Float(_) => Ok(LpcValue::Int(0)),
        other => bail!("cannot index {}", other.type_name()),
    }
}

fn index_set_value(container: LpcValue, index: LpcValue, value: LpcValue) -> Result<LpcValue> {
    match container {
        LpcValue::Array(mut values) => {
            let idx = match index.as_int() {
                Some(i) if i >= 0 => i as usize,
                _ => bail!("array index must be a non-negative integer"),
            };
            if idx >= values.len() {
                values.resize(idx + 1, LpcValue::Null);
            }
            values[idx] = value;
            Ok(LpcValue::Array(values))
        }
        LpcValue::Mapping(mut values) => {
            values.insert(mapping_key(&index), value);
            Ok(LpcValue::Mapping(values))
        }
        LpcValue::Null => {
            // MudOS: assigning into an uninitialized mapping var creates one.
            let mut values = IndexMap::new();
            values.insert(mapping_key(&index), value);
            Ok(LpcValue::Mapping(values))
        }
        other => bail!("cannot index-assign {}", other.type_name()),
    }
}

fn slice_value(value: LpcValue, start: LpcValue, end: LpcValue) -> Result<LpcValue> {
    match value {
        LpcValue::Array(values) => {
            let (start, end) = slice_bounds(&start, &end, values.len())?;
            Ok(LpcValue::Array(values[start..end].to_vec()))
        }
        LpcValue::String(value) => {
            let characters: Vec<char> = value.chars().collect();
            let (start, end) = slice_bounds(&start, &end, characters.len())?;
            Ok(LpcValue::String(characters[start..end].iter().collect()))
        }
        other => bail!("cannot slice {}", other.type_name()),
    }
}

fn slice_set_value(
    container: LpcValue,
    start: LpcValue,
    end: LpcValue,
    replacement: LpcValue,
) -> Result<LpcValue> {
    match container {
        LpcValue::String(value) => {
            let characters: Vec<char> = value.chars().collect();
            let (start, end) = slice_bounds(&start, &end, characters.len())?;
            let repl: Vec<char> = replacement.to_string().chars().collect();
            let mut result = Vec::with_capacity(characters.len() - (end - start) + repl.len());
            result.extend_from_slice(&characters[..start]);
            result.extend(repl);
            result.extend_from_slice(&characters[end..]);
            Ok(LpcValue::String(result.into_iter().collect()))
        }
        LpcValue::Array(values) => {
            let (start, end) = slice_bounds(&start, &end, values.len())?;
            let repl = match replacement {
                LpcValue::Array(items) => items,
                other => vec![other],
            };
            let mut result = Vec::with_capacity(values.len() - (end - start) + repl.len());
            result.extend_from_slice(&values[..start]);
            result.extend(repl);
            result.extend_from_slice(&values[end..]);
            Ok(LpcValue::Array(result))
        }
        other => bail!("cannot slice-assign {}", other.type_name()),
    }
}

fn normalized_index(index: LpcValue, length: usize) -> Result<usize> {
    let index = index.as_int().context("index must be an integer")?;
    let index = if index < 0 {
        length as i64 + index
    } else {
        index
    };
    if index < 0 {
        return Ok(length);
    }
    Ok(index as usize)
}

fn slice_bounds(start: &LpcValue, end: &LpcValue, length: usize) -> Result<(usize, usize)> {
    let start = if matches!(start, LpcValue::Null) {
        0
    } else {
        normalized_index(start.clone(), length)?.min(length)
    };
    let end = if matches!(end, LpcValue::Null) {
        length
    } else {
        normalized_index(end.clone(), length)?
            .saturating_add(1)
            .min(length)
    };
    Ok((start.min(end), end))
}

fn mapping_key(value: &LpcValue) -> String {
    match value {
        LpcValue::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn checked_target(target: usize, code: &[Op]) -> Result<usize> {
    if target <= code.len() {
        Ok(target)
    } else {
        bail!("invalid jump target {target}")
    }
}

fn boolean(value: bool) -> LpcValue {
    LpcValue::Int(i64::from(value))
}
