
use std::any::Any;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperandKind {
    Variable,
    Constant,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataType {
    Int,
    QuotedString,
    Identifier,
    Bool,
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

#[derive(Debug)]
pub struct IROperand {
    pub kind: OperandKind,
    pub data_type: DataType,
    pub src_loc: Range<usize>,
    pub val: Box<dyn Any>,
}

#[derive(Debug, Clone)]
pub struct IR {
    pub kind: IRKind,
    pub operands: Vec<usize>,
    pub abs_start: usize,
    pub size: usize,
}