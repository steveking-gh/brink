// Shared intermediate representation (IR) types for brink.
//
// This crate defines the data types that flow between the lineardb, irdb and
// engine pipeline stages.  IRKind enumerates every operation the compiler
// understands.  ParameterValue holds a typed runtime value (U64, I64, Integer,
// QuotedString, or Identifier) for each operand.  IROperand pairs a
// ParameterValue with its source location and its optional back-reference to
// the IR instruction that produced it.  IR bundles a kind, a source location,
// and a list of operand indices into a single instruction record.
//
// Order of operations: ir.rs is a shared library with no pipeline logic of
// its own.  lineardb populates LinIR records, irdb converts them into typed IR
// and IROperand values, and engine reads those values during iteration and
// execution.

use diags::Diags;
use parse_int::parse;
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
    Eq,
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
    /// Write N bytes (little-endian). N is the byte width: 1..=8.
    Wr(u8),
    Wrf,
    Wrs,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterValue {
    U64(u64),
    I64(i64),
    Integer(i64), // ambiguously U64 or I64, physically backed by i64
    QuotedString(String),
    Identifier(String),
    Unknown,
}

impl ParameterValue {
    pub fn data_type(&self) -> DataType {
        match self {
            ParameterValue::U64(_) => DataType::U64,
            ParameterValue::I64(_) => DataType::I64,
            ParameterValue::Integer(_) => DataType::Integer,
            ParameterValue::QuotedString(_) => DataType::QuotedString,
            ParameterValue::Identifier(_) => DataType::Identifier,
            ParameterValue::Unknown => DataType::Unknown,
        }
    }

    pub fn to_bool(&self) -> bool {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => (*v as u64) != 0,
            ParameterValue::U64(v) => *v != 0,
            _ => {
                panic!("Internal error: Invalid type conversion to bool");
            }
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => *v as u64,
            ParameterValue::U64(v) => *v,
            _ => {
                panic!("Internal error: Invalid type conversion to u64");
            }
        }
    }

    pub fn to_u64_mut(&mut self) -> &mut u64 {
        match self {
            ParameterValue::U64(v) => v,
            _ => {
                panic!("Internal error: Invalid type conversion to &mut u64");
            }
        }
    }

    pub fn to_i64(&self) -> i64 {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => *v,
            _ => {
                panic!("Internal error: Invalid type conversion to i64");
            }
        }
    }

    pub fn to_i64_mut(&mut self) -> &mut i64 {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => v,
            _ => {
                panic!("Internal error: Invalid type conversion to &mut i64");
            }
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            ParameterValue::QuotedString(s) => s,
            _ => {
                panic!("Internal error: Invalid type conversion to str");
            }
        }
    }

    pub fn to_identifier(&self) -> &str {
        match self {
            ParameterValue::Identifier(s) => s,
            _ => {
                panic!("Internal error: Invalid type conversion to identifier");
            }
        }
    }
}

#[derive(Debug)]
pub struct IROperand {
    /// Some(linear ID) of source operation if this operand is an output.
    /// None for constants.
    pub ir_lid: Option<usize>,
    pub src_loc: Range<usize>,
    pub is_constant: bool,
    pub val: ParameterValue,
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
                val,
            });
        }

        None
    }

    pub fn is_output_of(&self) -> Option<usize> {
        self.ir_lid
    }

    /// Converts the specified string into the specified type
    fn convert_type(
        sval: &str,
        data_type: DataType,
        src_loc: &Range<usize>,
        is_constant: bool,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        match data_type {
            DataType::QuotedString => {
                // Trim quotes and convert escape characters
                // For trimming, don't use trim_matches since that
                // will incorrectly strip trailing escaped quotes.
                return Some(ParameterValue::QuotedString(
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
                    let res = parse::<u64>(sval_no_u);
                    if let Ok(v) = res {
                        return Some(ParameterValue::U64(v));
                    } else {
                        let m = format!("Malformed integer operand {}", sval);
                        diags.err1("IR_1", &m, src_loc.clone());
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(ParameterValue::U64(0));
                }
            }

            DataType::I64 => {
                if is_constant {
                    // Strip the trailing 's' if any
                    let sval_no_i = sval.strip_suffix('i').unwrap_or(sval);
                    let res = parse::<i64>(sval_no_i);
                    if let Ok(v) = res {
                        return Some(ParameterValue::I64(v));
                    } else {
                        let m = format!("Malformed integer operand {}", sval);
                        diags.err1("IR_3", &m, src_loc.clone());
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(ParameterValue::I64(0));
                }
            }

            DataType::Integer => {
                if is_constant {
                    // We have to store Integer as a real Rust type.  Storing as i64
                    // is least surprising since expectations like 1 - 2 == -1 hold.
                    let res = parse::<i64>(sval);
                    if let Ok(v) = res {
                        return Some(ParameterValue::Integer(v));
                    } else {
                        let m = format!("Malformed integer operand {}", sval);
                        diags.err1("IR_4", &m, src_loc.clone());
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(ParameterValue::Integer(0));
                }
            }

            DataType::Identifier => {
                return Some(ParameterValue::Identifier(sval.to_string()));
            }
            DataType::Unknown => {
                let m = format!("Conversion failed for unknown type {}.", sval);
                diags.err1("IR_2", &m, src_loc.clone());
                return Some(ParameterValue::Unknown);
            }
        };
        None
    }

    pub fn clone_val(&self) -> ParameterValue {
        self.val.clone()
    }
}

#[derive(Debug, Clone)]
pub struct IR {
    pub kind: IRKind,
    pub operands: Vec<usize>,
    pub src_loc: Range<usize>,
}
