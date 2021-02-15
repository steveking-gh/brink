
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
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IRKind {
    Assert,
    NEq,
    GEq,
    LEq,
    DoubleEq,
    U64,
    Multiply,
    Divide,
    Add,
    Subtract,
    LogicalAnd,
    BitAnd,
    BitOr,
    LogicalOr,
    LeftShift,
    RightShift,
    SectionStart,
    SectionEnd,
    Abs,
    Img,
    Sec,
    Sizeof,
    Wrs,
}

#[derive(Debug)]
pub struct IROperand {
    /// Some(linear ID) of source operation if this operand is an output.
    /// None for constants.
    pub src_lid: Option<usize>,
    pub kind: OperandKind,
    pub data_type: DataType,
    pub src_loc: Range<usize>,
    pub val: Box<dyn Any>,
}

impl IROperand {
    pub fn clone_val_box(&self) -> Box<dyn Any> {
        match self.data_type {
            DataType::Int => { Box::new(self.val.downcast_ref::<u64>().unwrap().clone()) },
            DataType::QuotedString |
            DataType::Identifier => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
            DataType::Unknown => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
        }
    }
}

impl IROperand {
    pub fn to_bool(&self) -> bool {
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<u64>().unwrap() != 0 },
            _ => { assert!(false); false },
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<u64>().unwrap() },
            _ => { assert!(false); 0 },
        }
    }

    pub fn to_str(&self) -> &str {
        match self.data_type {
            DataType::QuotedString => { self.val.downcast_ref::<String>().unwrap() },
            _ => { assert!(false); "" },
        }
    }
    pub fn to_identifier(&self) -> &str {
        match self.data_type {
            DataType::Identifier => { self.val.downcast_ref::<String>().unwrap() },
            _ => { assert!(false); "" },
        }
    }

}

#[derive(Debug, Clone)]
pub struct IR {
    pub kind: IRKind,
    pub operands: Vec<usize>,
    pub src_loc: Range<usize>,
}