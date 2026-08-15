#[derive(Clone, Debug)]
pub struct ProgramAst {
    pub inherits: Vec<String>,
    pub globals: Vec<VariableDecl>,
    pub functions: Vec<FunctionDecl>,
}

#[derive(Clone, Debug)]
pub struct VariableDecl {
    pub name: String,
    pub initializer: Option<Expr>,
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
    Return(Option<Expr>),
    Empty,
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
    Unary {
        operator: UnaryOp,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnaryOp {
    Negate,
    Not,
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
}
