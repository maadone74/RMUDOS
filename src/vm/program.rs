use super::value::{ClassDef, LpcValue};
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
    Swap,
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
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Index,
    IndexSet,
    Slice,
    /// Replace a slice: stack is `base, start, end, value` → updated base.
    SliceSet,
    MakeArray(usize),
    MakeMapping(usize),
    Jump(usize),
    JumpIfFalse(usize),
    Call(String, usize),
    CallEfun(String, usize),
    /// Call `name` on an inherited program (MudOS `::name` / `foo::name`).
    CallInherit(Option<String>, String, usize),
    /// Pop args then callee; invoke function value.
    CallValue(usize),
    ThisObject,
    /// Pop `bound` args and push `(: name, bound... :)`.
    MakeNamedFunction(String, usize),
    /// Push `(: expr :)` with a compiled anonymous function body.
    MakeExprFunction(std::sync::Arc<FunctionInfo>),
    /// Runtime cast to the named LPC type.
    Cast(String),
    /// Begin a `catch` region; on error jump to handler pc with error string on stack.
    EnterCatch(usize),
    /// Successful end of catch body: discard value, push 0, clear catch frame.
    LeaveCatchSuccess,
    /// Push a new instance of the given class (fields initialized to 0).
    NewClass(Arc<ClassDef>),
    /// Pop class instance; push field value.
    MemberGet(String),
    /// Stack `instance, value` → store field, leave `value`.
    MemberSet(String),
    Return,
}

#[derive(Clone, Debug)]
pub struct FunctionInfo {
    pub name: String,
    pub parameters: Vec<String>,
    pub local_count: usize,
    pub code: Vec<Op>,
    pub source_line: usize,
    /// Object path of the program that defined this function body.
    /// Used so bare `::foo()` resolves against that file's inherits, not the leaf object's.
    pub defining_path: String,
}

impl FunctionInfo {
    pub fn arity(&self) -> usize {
        self.parameters.len()
    }
}

/// Rewrite `LoadGlobal`/`StoreGlobal` indices from one program layout to another.
/// Used when merging inherits and when `inherit::fun()` runs on a child object.
pub fn relocate_function(
    function: &FunctionInfo,
    relocation: &[usize],
) -> Result<FunctionInfo, String> {
    let mut function = function.clone();
    for operation in &mut function.code {
        match operation {
            Op::LoadGlobal(index) | Op::StoreGlobal(index) => {
                let mapped = relocation.get(*index).copied().ok_or_else(|| {
                    format!(
                        "global index {index} out of range ({} entries) in {}::{}",
                        relocation.len(),
                        function.defining_path,
                        function.name
                    )
                })?;
                *index = mapped;
            }
            Op::MakeExprFunction(inner) => {
                *inner = Arc::new(relocate_function(inner, relocation)?);
            }
            _ => {}
        }
    }
    Ok(function)
}

pub fn relocate_function_to_globals(
    function: &FunctionInfo,
    from_globals: &[String],
    to_globals: &[String],
) -> Result<FunctionInfo, String> {
    let mut relocation = Vec::with_capacity(from_globals.len());
    for name in from_globals {
        match to_globals.iter().position(|candidate| candidate == name) {
            Some(index) => relocation.push(index),
            None => {
                return Err(format!(
                    "inherited global {name} missing in object (from {})",
                    function.defining_path
                ));
            }
        }
    }
    relocate_function(function, &relocation)
}

#[derive(Clone, Debug)]
pub struct Program {
    pub path: String,
    pub inherits: Vec<String>,
    pub inherit_programs: Vec<Arc<Program>>,
    pub globals: Vec<String>,
    pub nosave_globals: Vec<bool>,
    pub classes: IndexMap<String, Arc<ClassDef>>,
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
            nosave_globals: Vec::new(),
            classes: IndexMap::new(),
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
