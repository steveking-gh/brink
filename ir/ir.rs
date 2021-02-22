use std::any::Any;
use std::ops::Range;
use diags::Diags;
use parse_int::parse;

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
    Abs,
    Add,
    Assert,
    BitAnd,
    BitOr,
    Divide,
    DoubleEq,
    GEq,
    Img,
    Label,
    LeftShift,
    LEq,
    LogicalAnd,
    LogicalOr,
    Multiply,
    NEq,
    RightShift,
    Sec,
    SectionEnd,
    SectionStart,
    Sizeof,
    Subtract,
    U64,
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

    pub fn new(src_lid: Option<usize>, sval: &str, src_loc: &Range<usize>, kind: OperandKind,
               data_type: DataType, diags: &mut Diags) -> Option<IROperand> {

        if let Some(val) = IROperand::convert_type(sval, data_type, kind, src_loc, diags) {
            return Some(IROperand { src_lid, src_loc: src_loc.clone(),
                        kind, data_type, val });
        }

        None
    }
    
    fn convert_type(sval: &str, data_type: DataType, kind: OperandKind,
                    src_loc: &Range<usize>, diags: &mut Diags) -> Option<Box<dyn Any>> {
        match data_type {
            DataType::QuotedString => {
                // Trim quotes and convert escape characters
                // For trimming, don't use trim_matches since that
                // will incorrectly strip trailing escaped quotes.
                return Some(Box::new(sval
                        .strip_prefix('\"').unwrap()
                        .strip_suffix('\"').unwrap()
                        .replace("\\\"", "\"")
                        .replace("\\n", "\n")
                        .replace("\\t", "\t")));
            }
            DataType::Int => {
                if kind == OperandKind::Constant {
                    let res = parse::<u64>(sval);
                    if let Ok(v) = res {
                        return Some(Box::new(v));
                    } else {
                        let m = format!("Malformed integer operand {}", sval);
                        diags.err1("IR_1", &m, src_loc.clone());
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(Box::new(0u64));
                }
            }
            DataType::Identifier => {
                return Some(Box::new(sval.to_string()));
            }
            DataType::Unknown => {
                let m = format!("Conversion failed for unknown type {}.", sval);
                diags.err1("IR_2", &m, src_loc.clone());
            }
        };
        None
    }


    pub fn clone_val_box(&self) -> Box<dyn Any> {
        match self.data_type {
            DataType::Int => { Box::new(self.val.downcast_ref::<u64>().unwrap().clone()) },
            DataType::QuotedString |
            DataType::Identifier => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
            DataType::Unknown => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
        }
    }

    pub fn to_bool(&self) -> bool {
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<u64>().unwrap() != 0 },
            _ => { panic!("Internal error: Invalid type conversion to bool"); },
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<u64>().unwrap() },
            _ => { panic!("Internal error: Invalid type conversion to u64"); },
        }
    }

    pub fn to_str(&self) -> &str {
        match self.data_type {
            DataType::QuotedString => { self.val.downcast_ref::<String>().unwrap() },
            _ => { panic!("Internal error: Invalid type conversion to str"); },
        }
    }
    pub fn to_identifier(&self) -> &str {
        match self.data_type {
            DataType::Identifier => { self.val.downcast_ref::<String>().unwrap() },
            _ => { panic!("Internal error: Invalid type conversion to identifier"); },
        }
    }

}

#[derive(Debug, Clone)]
pub struct IR {
    pub kind: IRKind,
    pub operands: Vec<usize>,
    pub src_loc: Range<usize>,
}