#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperandKind {
    TempVar,
    Immediate,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataType {
    Int,
    QuotedString,
    Identifier,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IRKind {
    Assert,
    Begin,
    EqEq,
    Int,
    Load,
    Multiply,
    Add,
    SectionStart,
    SectionEnd,
    Wrs,
}


