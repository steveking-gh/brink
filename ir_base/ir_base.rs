
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
    EqEq,
    Int,
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

impl IROperand {
    pub fn clone_val_box(&self) -> Box<dyn Any> {
        match self.data_type {
            DataType::Int => { Box::new(self.val.downcast_ref::<i64>().unwrap().clone()) },
            DataType::QuotedString |
            DataType::Identifier => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
            DataType::Bool =>  {Box::new(self.val.downcast_ref::<bool>().unwrap().clone())},
            DataType::Unknown => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
        }
    }
}

#[derive(Debug, Clone)]
pub struct IR {
    pub kind: IRKind,
    pub operands: Vec<usize>,
    pub src_loc: Range<usize>,
}