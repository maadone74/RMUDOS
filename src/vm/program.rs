use super::value::LpcValue;
use indexmap::IndexMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Op {
    Constant(LpcValue),
    LoadGlobal(usize),
    StoreGlobal(usize),
    LoadLocal(usize),
    StoreLocal(usize),
    Pop,
    Dup,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Negate,
    Not,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    Index,
    IndexSet,
    Slice,
    MakeArray(usize),
    MakeMapping(usize),
    Jump(usize),
    JumpIfFalse(usize),
    Call(String, usize),
    CallEfun(String, usize),
    ThisObject,
    Return,
}

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub name: String,
    pub parameters: Vec<String>,
    pub local_count: usize,
    pub code: Vec<Op>,
    pub source_line: usize,
}

impl FunctionInfo {
    pub fn arity(&self) -> usize {
        self.parameters.len()
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub path: String,
    pub inherits: Vec<String>,
    pub inherit_programs: Vec<Arc<Program>>,
    pub globals: Vec<String>,
    pub functions: IndexMap<String, FunctionInfo>,
    pub local_functions: IndexMap<String, FunctionInfo>,
}

impl Program {
    pub fn empty(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            inherits: Vec::new(),
            inherit_programs: Vec::new(),
            globals: Vec::new(),
            functions: IndexMap::new(),
            local_functions: IndexMap::new(),
        }
    }

    pub fn has_function(&self, name: &str) -> bool {
        self.local_functions.contains_key(name)
            || self
                .inherit_programs
                .iter()
                .any(|program| program.has_function(name))
    }
}
