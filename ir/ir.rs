use diags::Diags;
use parse_int::parse;
use std::any::Any;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    U64,
    I64,
    Integer, // ambiguously U64 or I64
    QuotedString,
    Identifier,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IRKind {
    Abs,
    Add,
    Align,
    Assert,
    BitAnd,
    BitOr,
    Divide,
    DoubleEq,
    GEq,
    I64,
    Img,
    Label,
    LeftShift,
    LEq,
    LogicalAnd,
    LogicalOr,
    Modulo,
    Multiply,
    NEq,
    SetSec,
    SetImg,
    SetAbs,
    Print,
    RightShift,
    Sec,
    SectionEnd,
    SectionStart,
    Sizeof,
    Subtract,
    ToI64,
    ToU64,
    U64,
    Wr8,
    Wr16,
    Wr24,
    Wr32,
    Wr40,
    Wr48,
    Wr56,
    Wr64,
    Wrf,
    Wrs,
}

#[derive(Debug)]
pub struct IROperand {
    /// Some(linear ID) of source operation if this operand is an output.
    /// None for constants.
    pub ir_lid: Option<usize>,
    pub src_loc: Range<usize>,
    pub is_constant: bool,
    pub data_type: DataType,
    pub val: Box<dyn Any>,
}

impl IROperand {
    pub fn new(
        ir_lid: Option<usize>,
        sval: &str,
        src_loc: &Range<usize>,
        data_type: DataType,
        is_constant: bool,
        diags: &mut Diags,
    ) -> Option<IROperand> {
        if let Some(val) = IROperand::convert_type(sval, data_type, src_loc, is_constant, diags) {
            return Some(IROperand {
                ir_lid,
                src_loc: src_loc.clone(),
                is_constant,
                data_type,
                val,
            });
        }

        None
    }

    pub fn is_output_of(&self) -> Option<usize> {
        return self.ir_lid;
    }

    /// Converts the specified string into the specified type
    fn convert_type(
        sval: &str,
        data_type: DataType,
        src_loc: &Range<usize>,
        is_constant: bool,
        diags: &mut Diags,
    ) -> Option<Box<dyn Any>> {
        match data_type {
            DataType::QuotedString => {
                // Trim quotes and convert escape characters
                // For trimming, don't use trim_matches since that
                // will incorrectly strip trailing escaped quotes.
                return Some(Box::new(
                    sval.strip_prefix('\"')
                        .unwrap()
                        .strip_suffix('\"')
                        .unwrap()
                        .replace("\\\"", "\"")
                        .replace("\\n", "\n")
                        .replace("\\0", "\0")
                        .replace("\\t", "\t"),
                ));
            }
            DataType::U64 => {
                if is_constant {
                    // Strip the trailing 'u' if any
                    let sval_no_u = sval.strip_suffix('u').unwrap_or(sval);
                    let res = parse::<u64>(&sval_no_u);
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

            DataType::I64 => {
                if is_constant {
                    // Strip the trailing 's' if any
                    let sval_no_i = sval.strip_suffix('i').unwrap_or(sval);
                    let res = parse::<i64>(sval_no_i);
                    if let Ok(v) = res {
                        return Some(Box::new(v));
                    } else {
                        let m = format!("Malformed integer operand {}", sval);
                        diags.err1("IR_3", &m, src_loc.clone());
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(Box::new(0i64));
                }
            }

            DataType::Integer => {
                if is_constant {
                    // We have to store Integer as a real Rust type.  Storing as i64
                    // is least surprising since expectations like 1 - 2 == -1 hold.
                    let res = parse::<i64>(sval);
                    if let Ok(v) = res {
                        return Some(Box::new(v));
                    } else {
                        let m = format!("Malformed integer operand {}", sval);
                        diags.err1("IR_3", &m, src_loc.clone());
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(Box::new(0i64));
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
            DataType::U64 => { Box::new(self.val.downcast_ref::<u64>().unwrap().clone()) },
            DataType::Integer | // Integer stored as i64
            DataType::I64 => { Box::new(self.val.downcast_ref::<i64>().unwrap().clone()) },
            DataType::QuotedString |
            DataType::Identifier => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
            DataType::Unknown => {Box::new(self.val.downcast_ref::<String>().unwrap().clone())},
        }
    }

    pub fn to_bool(&self) -> bool {
        match self.data_type {
            DataType::Integer | // Integer stored as i64
            DataType::I64 => { (*self.val.downcast_ref::<i64>().unwrap() as u64) != 0 },
            DataType::U64 => { *self.val.downcast_ref::<u64>().unwrap() != 0 },
            _ => { panic!("Internal error: Invalid type conversion to bool"); },
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self.data_type {
            DataType::Integer => *self.val.downcast_ref::<i64>().unwrap() as u64,
            DataType::U64 => *self.val.downcast_ref::<u64>().unwrap(),
            _ => {
                panic!("Internal error: Invalid type conversion to u64");
            }
        }
    }

    pub fn to_i64(&self) -> i64 {
        match self.data_type {
            DataType::Integer | DataType::I64 => *self.val.downcast_ref::<i64>().unwrap(),
            _ => {
                panic!("Internal error: Invalid type conversion to i64");
            }
        }
    }

    pub fn to_str(&self) -> &str {
        match self.data_type {
            DataType::QuotedString => self.val.downcast_ref::<String>().unwrap(),
            _ => {
                panic!("Internal error: Invalid type conversion to str");
            }
        }
    }
    pub fn to_identifier(&self) -> &str {
        match self.data_type {
            DataType::Identifier => self.val.downcast_ref::<String>().unwrap(),
            _ => {
                panic!("Internal error: Invalid type conversion to identifier");
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct IR {
    pub kind: IRKind,
    pub operands: Vec<usize>,
    pub src_loc: Range<usize>,
}
