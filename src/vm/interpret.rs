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
        let function = Self::find_function(&program, name)
            .with_context(|| format!("{} has no function {name}", program.path))?;
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
        if self.call_depth >= 128 {
            bail!("maximum call depth exceeded");
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
        if arguments.len() < function.arity() {
            bail!(
                "{} expects {} arguments, received {}",
                function.name,
                function.arity(),
                arguments.len()
            );
        }
        let mut locals = vec![LpcValue::Null; function.local_count.max(arguments.len())];
        for (slot, value) in locals.iter_mut().zip(arguments) {
            *slot = value;
        }
        let mut stack = Vec::new();
        let mut instruction = 0;
        while instruction < function.code.len() {
            self.cost += 1;
            if self.cost > self.max_cost {
                bail!("maximum evaluation cost of {} exceeded", self.max_cost);
            }
            let operation = function.code[instruction].clone();
            instruction += 1;
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
                Op::Add => binary(&mut stack, add)?,
                Op::Subtract => binary(&mut stack, |left, right| {
                    numeric(left, right, |a, b| a - b, |a, b| a - b)
                })?,
                Op::Multiply => binary(&mut stack, |left, right| {
                    numeric(left, right, |a, b| a * b, |a, b| a * b)
                })?,
                Op::Divide => binary(&mut stack, divide)?,
                Op::Modulo => binary(&mut stack, |left, right| {
                    let divisor = right.as_int().context("modulo divisor must be an integer")?;
                    if divisor == 0 {
                        bail!("division by zero");
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
                Op::CallEfun(name, count) => {
                    let arguments = pop_arguments(&mut stack, count)?;
                    let efun = self
                        .world
                        .efuns
                        .get(&name)
                        .with_context(|| format!("unknown efun {name}"))?;
                    stack.push(efun(self, arguments)?);
                }
                Op::ThisObject => stack.push(LpcValue::Object(self.current_object.clone())),
                Op::Return => return Ok(stack.pop().unwrap_or(LpcValue::Null)),
            }
        }
        Ok(LpcValue::Null)
    }
}

fn pop(stack: &mut Vec<LpcValue>) -> Result<LpcValue> {
    stack.pop().context("stack underflow")
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
        (left, right) => numeric(left, right, |a, b| a + b, |a, b| a + b),
    }
}

fn divide(left: LpcValue, right: LpcValue) -> Result<LpcValue> {
    if matches!(&right, LpcValue::Int(0))
        || matches!(&right, LpcValue::Float(value) if *value == 0.0)
    {
        bail!("division by zero");
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
                .map(|character| LpcValue::String(character.to_string()))
                .unwrap_or(LpcValue::Null))
        }
        LpcValue::Mapping(values) => Ok(values
            .get(&mapping_key(&index))
            .cloned()
            .unwrap_or(LpcValue::Null)),
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
