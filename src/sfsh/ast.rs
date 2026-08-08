#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamOp {
    Def(String, bool),
    Assign(String, bool),
    Err(String, bool),
    Alt(String, bool),
    Len,
    Off(String),
    OffLen(String, String),
    RemSF(String),
    RemLF(String),
    RemSB(String),
    RemLB(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordPart {
    Lit(String),
    SQuote(String),
    DQuote(Vec<WordPart>),
    Tilde(String),
    Param(String, Option<ParamOp>),
    Cmd(String),
    Arith(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word(pub Vec<WordPart>);

#[derive(Debug, Clone)]
pub enum Redirect {
    Out(Option<u32>, Word),
    In(Option<u32>, Word),
    Append(Option<u32>, Word),
    Here(Option<u32>, Word, bool, bool, Option<String>),
    DupOut(Option<u32>, Word),
    DupIn(Option<u32>, Word),
    ReadWrite(Option<u32>, Word),
    OutErr(Word),
}

#[derive(Debug, Clone)]
pub enum CondExpr {
    Or(Box<CondExpr>, Box<CondExpr>),
    And(Box<CondExpr>, Box<CondExpr>),
    Not(Box<CondExpr>),
    Paren(Box<CondExpr>),
    Unary(String, Word),
    Binary(String, Word, Word),
}

#[derive(Debug, Clone)]
pub enum Command {
    Simple(Vec<(String, Word)>, Vec<Word>, Vec<Redirect>),
    Pipeline(Vec<Command>),
    List(Vec<(Command, Option<String>)>),
    If(Box<Command>, Box<Command>, Option<Box<Command>>),
    For(String, Vec<Word>, Box<Command>),
    While(Box<Command>, Box<Command>),
    Until(Box<Command>, Box<Command>),
    Case(Word, Vec<(Vec<Word>, Command)>),
    Function(String, Box<Command>),
    Subshell(Box<Command>),
    Brace(Box<Command>),
    Not(Box<Command>),
    Cond(Box<CondExpr>),
    Empty,
}
