#[derive(Clone, Debug)]
pub struct ProgramAst {
    pub inherits: Vec<String>,
    pub classes: Vec<ClassDecl>,
    pub globals: Vec<VariableDecl>,
    pub functions: Vec<FunctionDecl>,
}

#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub name: String,
    pub fields: Vec<String>,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct VariableDecl {
    pub name: String,
    pub initializer: Option<Expr>,
    pub nosave: bool,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub parameters: Vec<String>,
    pub body: Stmt,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Block(Vec<Stmt>),
    Variable(VariableDecl),
    Expression(Expr),
    If {
        condition: Expr,
        then_branch: Box<Stmt>,
        else_branch: Option<Box<Stmt>>,
    },
    While {
        condition: Expr,
        body: Box<Stmt>,
    },
    For {
        init: Option<Expr>,
        condition: Option<Expr>,
        step: Option<Expr>,
        body: Box<Stmt>,
    },
    /// `foreach (x in coll)` or `foreach (k, v in mapping)`.
    Foreach {
        variables: Vec<String>,
        collection: Expr,
        body: Box<Stmt>,
    },
    Switch {
        value: Expr,
        cases: Vec<SwitchCase>,
    },
    Break,
    Continue,
    Return(Option<Expr>),
    Empty,
}

#[derive(Clone, Debug)]
pub struct SwitchCase {
    /// `None` label means `default`. Multiple labels = fall-through grouping.
    pub labels: Vec<Option<CaseLabel>>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum CaseLabel {
    Value(Expr),
    Range(Expr, Expr),
}

#[derive(Clone, Debug)]
pub enum Expr {
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Expr>),
    Mapping(Vec<(Expr, Expr)>),
    Variable(String),
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
    /// C/LPC comma operator: evaluate left, keep right.
    Comma {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        operator: UnaryOp,
        operand: Box<Expr>,
    },
    /// Postfix `x++` / `x--`.
    Postfix {
        operator: PostfixOp,
        operand: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: BinaryOp,
        right: Box<Expr>,
    },
    Conditional {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    Call {
        name: String,
        arguments: Vec<Expr>,
    },
    Index {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        value: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    /// MudOS `(: name :)` / `(: name, bound... :)` function pointer.
    FunctionalNamed {
        name: String,
        bound: Vec<Expr>,
    },
    /// MudOS `(: expression :)` with `$1`…`$n` placeholders.
    FunctionalExpr {
        body: Box<Expr>,
    },
    /// `$n` inside a functional expression (1-based).
    DollarArg(usize),
    /// `(type)expr` cast.
    Cast {
        type_name: String,
        value: Box<Expr>,
    },
    /// `::name(args)` or `inherit::name(args)`.
    InheritCall {
        inherit: Option<String>,
        name: String,
        arguments: Vec<Expr>,
    },
    /// Call a function value: `fun(args)` / `(*fun)(args)`.
    CallValue {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    /// MudOS `catch(expr)` — returns 0 on success, error string on failure.
    Catch(Box<Expr>),
    /// MudOS class member access `obj->field` (no call).
    Member {
        object: Box<Expr>,
        field: String,
    },
    /// `new(class Name)` — allocate a class instance.
    NewClass {
        class_name: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOp {
    Negate,
    Not,
    /// MudOS `*fun` function-pointer dereference (runtime no-op for function values).
    Deref,
    BitNot,
    PreIncrement,
    PreDecrement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostfixOp {
    Increment,
    Decrement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
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
}
