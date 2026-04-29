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
use diags::SourceSpan;
use parse_int::parse;

/// Region properties bound to a section via `section NAME in REGION`.
/// Stored on IRDb; consumed by LayoutPhase and later execution phases.
/// Carries the region name and declaration source location so that every
/// error site can report which region was violated without extra lookups.
#[derive(Clone, Debug)]
pub struct RegionBinding {
    pub addr: u64,
    pub size: u64,
    /// The region name as written in source, e.g. "FLASH".
    pub name: String,
    /// Source location of the region declaration for diagnostic labels.
    pub src_loc: SourceSpan,
}

/// The effective region constraint for a section: the geometric intersection
/// of all ancestor region bindings plus the section's own direct binding.
/// contributors holds each RegionBinding that narrowed the intersection,
/// outermost first, for use in EXEC_73 backtrace diagnostics.
#[derive(Clone, Debug)]
pub struct EffectiveRegion {
    /// Geometric intersection of all applicable regions.
    pub binding: RegionBinding,
    /// All applicable regions, outermost first.
    pub contributors: Vec<RegionBinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    U64,
    I64,
    Integer, // ambiguously U64 or I64
    QuotedString,
    Identifier,
    /// Output type of an extension call.  All type checks reject Extension
    /// except for the ExtensionCall/ExtensionCallSection IR output slot.
    Extension,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IRKind {
    Addr,
    Add,
    Align,
    Assert,
    BitAnd,
    BitOr,
    Const,
    Divide,
    DoubleEq,
    Eq,
    /// Declares a const with no value: `const NAME;`.
    /// Operands: [name_identifier]
    ConstDeclare,
    /// Marks the start of an `if` block.
    /// Operands: [condition_expr_output]
    IfBegin,
    /// Marks the transition from the then-body to the else-body.
    /// Operands: []
    ElseBegin,
    /// Marks the end of an if/else construct.
    /// Operands: []
    IfEnd,
    /// Bare assignment inside an if/else body: `NAME = expr;`
    /// Operands: [name_identifier, rhs_expr_output]
    BareAssign,
    /// Operands: [name, arg0..., output]
    /// All extension calls use this form.  The engine resolves Slice-kinded
    /// params to ParamArg::Slice by consulting cached_params in the registry.
    ExtensionCall,
    GEq,
    Gt,
    I64,
    AddrOffset,
    Label,
    LeftShift,
    LEq,
    Lt,
    LogicalAnd,
    LogicalOr,
    Modulo,
    Multiply,
    NEq,
    BuiltinOutputAddr,
    BuiltinOutputSize,
    BuiltinVersionMajor,
    BuiltinVersionMinor,
    BuiltinVersionPatch,
    BuiltinVersionString,
    SetSecOffset,
    SetAddrOffset,
    SetAddr,
    SetFileOffset,
    Print,
    RightShift,
    SecOffset,
    FileOffset,
    SectionEnd,
    SectionStart,
    Sizeof,
    SizeofExt,
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
    /// Placeholder value for the output slot of an extension call.
    Extension,
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
            ParameterValue::Extension => DataType::Extension,
            ParameterValue::Unknown => DataType::Unknown,
        }
    }

    pub fn to_bool(&self) -> Option<bool> {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => Some((*v as u64) != 0),
            ParameterValue::U64(v) => Some(*v != 0),
            _ => None,
        }
    }

    pub fn to_u64(&self) -> u64 {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => *v as u64,
            ParameterValue::U64(v) => *v,
            _ => {
                panic!("Internal error: Invalid type conversion from {:?} to u64", self);
            }
        }
    }

    pub fn to_u64_mut(&mut self) -> &mut u64 {
        match self {
            ParameterValue::U64(v) => v,
            _ => {
                panic!("Internal error: Invalid type conversion from {:?} to &mut u64", self);
            }
        }
    }

    pub fn to_i64(&self) -> i64 {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => *v,
            _ => {
                panic!("Internal error: Invalid type conversion from {:?} to i64", self);
            }
        }
    }

    pub fn to_i64_mut(&mut self) -> &mut i64 {
        match self {
            ParameterValue::I64(v) | ParameterValue::Integer(v) => v,
            _ => {
                panic!("Internal error: Invalid type conversion from {:?} to &mut i64", self);
            }
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            ParameterValue::QuotedString(s) => s,
            _ => {
                panic!("Internal error: Invalid type conversion from {:?} to str", self);
            }
        }
    }

    pub fn identifier_to_str(&self) -> &str {
        match self {
            ParameterValue::Identifier(s) => s,
            _ => {
                panic!("Internal error: Invalid type conversion from {:?} to identifier", self);
            }
        }
    }
}

/// Strip a trailing K/M/G magnitude suffix from a numeric string.
/// Returns the stripped string and the magnitude multiplier.
pub fn strip_kmg(s: &str) -> (&str, u64) {
    match s.as_bytes().last() {
        Some(b'K') => (&s[..s.len() - 1], 1024),
        Some(b'M') => (&s[..s.len() - 1], 1024 * 1024),
        Some(b'G') => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        _ => (s, 1),
    }
}

#[derive(Debug)]
pub struct IROperand {
    /// The linear ID of the IR instruction whose output this operand carries,
    /// or None if this is an immediate (literal) operand with no producing
    /// instruction.
    pub ir_lid: Option<usize>,
    /// Byte range in the source file that produced this operand, used for
    /// error reporting.
    pub src_loc: SourceSpan,
    /// True if this operand holds a literal value parsed directly from source
    /// (e.g. a numeric constant or quoted string).  False if this is the
    /// output placeholder of an IR instruction whose value is computed at
    /// engine time.
    pub is_immediate: bool,
    /// The typed runtime value of this operand.  For immediate operands this
    /// is parsed from the source literal; for output placeholders it is
    /// initialized to a zero-equivalent and overwritten during execution.
    pub val: ParameterValue,
    /// Named-argument parameter name from the call site, if the caller used
    /// `name=value` syntax.  None for positional arguments.
    pub param_name: Option<String>,
}

impl IROperand {
    pub fn new(
        ir_lid: Option<usize>,
        sval: &str,
        src_loc: &SourceSpan,
        data_type: DataType,
        is_immediate: bool,
        diags: &mut Diags,
    ) -> Option<IROperand> {
        if let Some(val) = IROperand::convert_type(sval, data_type, src_loc, is_immediate, diags) {
            return Some(IROperand {
                ir_lid,
                src_loc: src_loc.clone(),
                is_immediate,
                val,
                param_name: None,
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
        src_loc: &SourceSpan,
        is_immediate: bool,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        match data_type {
            DataType::QuotedString => {
                if !is_immediate {
                    // Output operand of a string-typed Const IR.  The resolved
                    // value is stored in const_values and substituted before
                    // this placeholder is ever read, so an empty string is fine.
                    return Some(ParameterValue::QuotedString(String::new()));
                }
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
            DataType::Extension => {
                return Some(ParameterValue::Extension);
            }
            DataType::U64 => {
                if is_immediate {
                    let sval_no_u = sval.strip_suffix('u').unwrap_or(sval);
                    let (sval_base, mult) = strip_kmg(sval_no_u);
                    match parse::<u64>(sval_base).ok().and_then(|v| v.checked_mul(mult)) {
                        Some(v) => return Some(ParameterValue::U64(v)),
                        None => {
                            let m = format!("Malformed integer operand {}", sval);
                            diags.err1("IR_1", &m, src_loc.clone());
                        }
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(ParameterValue::U64(0));
                }
            }

            DataType::I64 => {
                if is_immediate {
                    let sval_no_i = sval.strip_suffix('i').unwrap_or(sval);
                    let (sval_base, mult) = strip_kmg(sval_no_i);
                    match parse::<i64>(sval_base).ok().and_then(|v| v.checked_mul(mult as i64)) {
                        Some(v) => return Some(ParameterValue::I64(v)),
                        None => {
                            let m = format!("Malformed integer operand {}", sval);
                            diags.err1("IR_3", &m, src_loc.clone());
                        }
                    }
                } else {
                    // We don't know variable value, so initialize to zero
                    return Some(ParameterValue::I64(0));
                }
            }

            DataType::Integer => {
                if is_immediate {
                    // Store as i64: expectations like 1 - 2 == -1 hold.
                    let (sval_base, mult) = strip_kmg(sval);
                    match parse::<i64>(sval_base).ok().and_then(|v| v.checked_mul(mult as i64)) {
                        Some(v) => return Some(ParameterValue::Integer(v)),
                        None => {
                            let m = format!("Malformed integer operand {}", sval);
                            diags.err1("IR_4", &m, src_loc.clone());
                        }
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
    pub src_loc: SourceSpan,
}

/// All compile-time constants exposed as Brink built-in variables.
///
/// Call `ConstBuiltins::init()` once at process startup before any built-in
/// variable is accessed.  Add new compile-time builtins as fields here as the
/// language grows.
pub struct ConstBuiltins {
    pub brink_version_string: &'static str,
    pub brink_version_major: u64,
    pub brink_version_minor: u64,
    pub brink_version_patch: u64,
}

impl ConstBuiltins {
    /// Initialize all compile-time built-in constants.
    /// This is deterministic and can be called multiple times safely.
    pub fn init() {
        // No-op. initialization is deterministic and done at each get().
    }

    pub fn from_version_str(version: &'static str) -> Self {
        let mut parts = version.splitn(3, '.');
        ConstBuiltins {
            brink_version_string: version,
            brink_version_major: parts.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            brink_version_minor: parts.next().and_then(|s| s.parse().ok()).unwrap_or(0),
            brink_version_patch: parts.next().and_then(|s| s.parse().ok()).unwrap_or(0),
        }
    }

    pub fn get() -> Self {
        Self::from_version_str(env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::ConstBuiltins;

    #[test]
    fn parse_version_string_valid() {
        let b = ConstBuiltins::from_version_str("4.5.6");
        assert_eq!(b.brink_version_string, "4.5.6");
        assert_eq!(b.brink_version_major, 4);
        assert_eq!(b.brink_version_minor, 5);
        assert_eq!(b.brink_version_patch, 6);
    }

    #[test]
    fn parse_version_string_malformed_minor() {
        let b = ConstBuiltins::from_version_str("10.x");
        assert_eq!(b.brink_version_string, "10.x");
        assert_eq!(b.brink_version_major, 10);
        assert_eq!(b.brink_version_minor, 0);
        assert_eq!(b.brink_version_patch, 0);
    }

    #[test]
    fn parse_version_string_partial_only() {
        let b = ConstBuiltins::from_version_str("7");
        assert_eq!(b.brink_version_string, "7");
        assert_eq!(b.brink_version_major, 7);
        assert_eq!(b.brink_version_minor, 0);
        assert_eq!(b.brink_version_patch, 0);
    }

    #[test]
    fn parse_version_string_nonnumeric() {
        let b = ConstBuiltins::from_version_str("foo.bar.baz");
        assert_eq!(b.brink_version_string, "foo.bar.baz");
        assert_eq!(b.brink_version_major, 0);
        assert_eq!(b.brink_version_minor, 0);
        assert_eq!(b.brink_version_patch, 0);
    }
}
