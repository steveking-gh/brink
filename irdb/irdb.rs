// Typed IR construction and validation for brink.
//
// IRDb is the third stage of the compiler pipeline.  It consumes the flat
// LinIR and LinOperand records from LayoutDb and converts them into fully
// typed IR and IROperand values.  String operand values are parsed into their
// native Rust types (u64, i64, etc.) and each operand's DataType is resolved
// by recursively inspecting the expression tree.  IRDb also validates operand
// counts, type compatibility, and file-path operands (checking that referenced
// files exist and are readable), reporting any errors through Diags.
//
// Order of operations: irdb runs after lineardb.  Its output — an IRDb
// containing ir_vec, parms and file metadata — is consumed by engine.

use constdb::ConstDb;
use diags::Diags;
use diags::SourceSpan;
use layoutdb::LayoutDb;
use linearizer::{LinIR, LinOperand};

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

use ext::ExtensionRegistry;
use ir::{ConstBuiltins, DataType, IR, IRKind, IROperand, ParameterValue};
use parse_int::parse;
use std::{
    collections::{HashMap, HashSet},
    fs,
    ops::Range,
    path::Path,
    path::PathBuf,
};
use symtable::SymbolTable;

pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub src_loc: SourceSpan,
}

pub struct IRDb {
    pub ir_vec: Vec<IR>,
    pub parms: Vec<IROperand>,

    /// Map a file path to the file info object
    pub files: HashMap<String, FileInfo>,

    /// The optional absolute starting address specified
    /// in the output statement.  Zero by default.
    pub start_addr: u64,

    /// Maps an identifier to the (start,stop) indices in the ir_vec.
    /// Used for items with a size (potentially zero) such as sections.
    pub sized_locs: HashMap<String, std::ops::Range<usize>>,

    /// Maps an identifier to its starting index in the ir_vec.
    /// Used for items that are addressable, including sections and labels
    pub addressed_locs: HashMap<String, usize>,

    /// Symbol table tracking every compile-time const from declaration through use.
    pub symbol_table: SymbolTable,

    /// Name of the section designated by the `output` statement.
    /// Used by the engine to evaluate `__OUTPUT_SIZE` and `__OUTPUT_ADDR`.
    pub output_sec_str: String,
}

/// Error returned by `calc_u64_op` / `calc_i64_op` before a diagnostic is emitted.
enum CalcErr {
    /// Arithmetic overflow or underflow; carries a human-readable message.
    Overflow(String),
    /// Division or modulo by zero.
    DivByZero,
}

impl IRDb {
    /// Returns the value of the specified operand for the specified IR.
    /// The operand number is for the *IR*, not the absolute operand
    /// index in the central operands vector.
    pub fn get_opnd_as_identifier(&self, ir: &IR, opnd_num: usize) -> &str {
        let &op_num = ir.operands.get(opnd_num).unwrap();
        let opnd = self.parms.get(op_num).unwrap();
        opnd.val.to_identifier()
    }

    pub fn get_operand_ir_lid(&self, opnd_num: usize) -> Option<usize> {
        self.parms.get(opnd_num).unwrap().ir_lid
    }

    /// Get the datatype of the referenced operand by recursively inspecting
    /// the input operands.
    /// Returns None on error
    fn get_operand_data_type_r(
        depth: usize,
        lop_num: usize,
        lin_db: &LayoutDb,
        symbol_table: &SymbolTable,
        diags: &mut Diags,
    ) -> Option<DataType> {
        trace!(
            "IRDb::get_operand_data_type_r: Enter at depth {} for lop number {}",
            depth, lop_num
        );
        let lop = &lin_db.operand_vec[lop_num];
        let mut data_type = None;

        match lop {
            linearizer::LinOperand::Output { ir_lid, .. } => {
                let lin_ir = &lin_db.ir_vec[*ir_lid];
                match lin_ir.op {
                    IRKind::ExtensionCall
                    | IRKind::ExtensionCallRanged
                    | IRKind::ExtensionCallSection => return Some(DataType::Extension),

                    // Arithmetic and bitwise ops: output type matches input types.
                    IRKind::Add
                    | IRKind::Subtract
                    | IRKind::Multiply
                    | IRKind::Divide
                    | IRKind::Modulo
                    | IRKind::BitOr
                    | IRKind::BitAnd
                    | IRKind::LeftShift
                    | IRKind::RightShift => {
                        // Expect 2 input operands and 1 output operand.
                        assert!(lin_ir.operand_vec.len() == 3);
                        assert!(lin_ir.operand_vec[2] == lop_num);
                        let lhs_num = lin_ir.operand_vec[0];
                        let rhs_num = lin_ir.operand_vec[1];

                        let lhs_opt = Self::get_operand_data_type_r(
                            depth + 1,
                            lhs_num,
                            lin_db,
                            symbol_table,
                            diags,
                        );
                        if let Some(lhs_dt) = lhs_opt {
                            let rhs_opt = Self::get_operand_data_type_r(
                                depth + 1,
                                rhs_num,
                                lin_db,
                                symbol_table,
                                diags,
                            );
                            if let Some(rhs_dt) = rhs_opt {
                                if lhs_dt == rhs_dt {
                                    let allowed = [DataType::I64, DataType::U64, DataType::Integer];
                                    if !allowed.contains(&lhs_dt) {
                                        let msg = format!(
                                            "Error, found data type '{:?}', but operation '{:?}' requires one of {:?}.",
                                            lhs_dt, lin_ir.op, allowed
                                        );
                                        diags.err1("IRDB_2", &msg, lin_ir.src_loc.clone());
                                    } else {
                                        data_type = Some(lhs_dt);
                                    }
                                } else {
                                    let mut dt_ok = false;
                                    // Attempt to reconcile Integer with a typed value.
                                    if rhs_dt == DataType::Integer {
                                        if [DataType::I64, DataType::U64, DataType::Integer]
                                            .contains(&lhs_dt)
                                        {
                                            dt_ok = true;
                                            data_type = Some(lhs_dt);
                                        }
                                    } else if lhs_dt == DataType::Integer
                                        && [DataType::I64, DataType::U64].contains(&rhs_dt)
                                    {
                                        dt_ok = true;
                                        data_type = Some(rhs_dt);
                                    }
                                    if !dt_ok {
                                        let msg = format!(
                                            "Error, data type mismatch in input operands.  Left is {:?}, right is {:?}.",
                                            lhs_dt, rhs_dt
                                        );
                                        diags.err1("IRDB_1", &msg, lin_ir.src_loc.clone());
                                    }
                                }
                            }
                        }
                    }

                    IRKind::ToI64 => return Some(DataType::I64),
                    IRKind::BuiltinVersionString => return Some(DataType::QuotedString),
                    // All other output-producing ops yield U64.
                    _ => return Some(DataType::U64),
                }
            }

            linearizer::LinOperand::Literal { tok, sval, .. } => {
                match tok {
                    // Literals typed directly at the site of the token.
                    ast::LexToken::U64 => data_type = Some(DataType::U64),
                    ast::LexToken::I64 => data_type = Some(DataType::I64),
                    ast::LexToken::Integer => data_type = Some(DataType::Integer),
                    ast::LexToken::QuotedString => data_type = Some(DataType::QuotedString),
                    ast::LexToken::Label => data_type = Some(DataType::Identifier),
                    ast::LexToken::Namespace => data_type = Some(DataType::Identifier),
                    ast::LexToken::BuiltinVersionString => data_type = Some(DataType::QuotedString),
                    ast::LexToken::Identifier => {
                        // If this identifier is a resolved const, return the const's type.
                        if let Some(cv) = symbol_table.get(sval.as_str()) {
                            data_type = Some(cv.data_type());
                        } else {
                            data_type = Some(DataType::Identifier);
                        }
                    }
                    _ => {
                        panic!("Literal operand with unexpected token {:?}", tok);
                    }
                }
            }
        };

        trace!(
            "IRDb::get_operand_data_type_r: Exit from depth {}, lop {} is {:?}",
            depth, lop_num, data_type
        );
        data_type
    }

    /// Process untyped linear operands into real IR operands
    fn process_lin_operands(&mut self, lin_db: &LayoutDb, diags: &mut Diags) -> bool {
        trace!("IRDb::process_lin_operands: Enter");

        let mut result = true;
        for (lop_num, lop) in lin_db.operand_vec.iter().enumerate() {
            // Const substitution: replace Identifier literals that name a resolved const
            // with the const's typed value so irdb never sees bare Identifier operands
            // for consts.
            if let linearizer::LinOperand::Literal {
                tok: ast::LexToken::Identifier,
                sval,
                src_loc,
            } = lop
                && let Some(const_val) = self.symbol_table.get(sval.as_str()).cloned()
            {
                self.symbol_table.mark_used(sval.as_str());
                self.parms.push(IROperand {
                    ir_lid: None,
                    src_loc: src_loc.clone(),
                    is_immediate: true,
                    val: const_val,
                });
                continue;
            }

            let dt_opt =
                Self::get_operand_data_type_r(0, lop_num, lin_db, &self.symbol_table, diags);
            if dt_opt.is_none() {
                return false; // error case, just give up
            }

            let data_type = dt_opt.unwrap();

            // Destructure fields needed by IROperand::new.  Output operands carry no
            // sval (the engine initializes their value at execution time), so pass "".
            let (ir_lid, sval, src_loc, is_immediate) = match lop {
                linearizer::LinOperand::Literal { sval, src_loc, .. } => {
                    (None, sval.as_str(), src_loc, true)
                }
                linearizer::LinOperand::Output { ir_lid, src_loc } => {
                    (Some(*ir_lid), "", src_loc, false)
                }
            };

            // Convert the string literal to a typed value; fails on malformed input.
            let opnd = IROperand::new(ir_lid, sval, src_loc, data_type, is_immediate, diags);
            if let Some(opnd) = opnd {
                self.parms.push(opnd);
            } else {
                // keep processing to return more type conversion errors, if any
                result = false;
            }
        }

        trace!("IRDb::process_lin_operands: Exit({})", result);
        result
    }

    // Print accepts most expressions without side effects
    // TODO add the restrictions that do exist, e.g. no identifiers
    fn validate_string_expr_operands(&self, _ir: &IR, _diags: &mut Diags) -> bool {
        true
    }

    fn validate_const_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 3 {
            let m = format!(
                "'{:?}' expressions must have 3 operands (identifier, =, value), but found {}.",
                ir.kind, len
            );
            diags.err1("IRDB_16", &m, ir.src_loc.clone());
            return false;
        }
        let identifier_opnd = &self.parms[ir.operands[0]];
        if identifier_opnd.val.data_type() != DataType::Identifier {
            let m = format!(
                "'{:?}' First const operand must be an identifier, found '{:?}'.",
                ir.kind,
                identifier_opnd.val.data_type()
            );
            diags.err2(
                "IRDB_17",
                &m,
                ir.src_loc.clone(),
                identifier_opnd.src_loc.clone(),
            );
            return false;
        }
        true
    }

    // Validate write file operands
    fn validate_wrf_operands(&mut self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 {
            let m = format!(
                "'{:?}' statements must have 1 operand, but found {}.",
                ir.kind, len
            );
            diags.err1("IRDB_10", &m, ir.src_loc.clone());
            return false;
        }

        let path_opnd = &self.parms[ir.operands[0]];
        if path_opnd.val.data_type() != DataType::QuotedString {
            let m = format!(
                "'{:?}' operand must be a file path in \
                    double-quotes, found '{:?}'.",
                ir.kind,
                path_opnd.val.data_type()
            );
            diags.err2("IRDB_11", &m, ir.src_loc.clone(), path_opnd.src_loc.clone());
            return false;
        }

        let path_str = path_opnd.val.to_str();
        let path = Path::new(path_str);

        // Determine if we already know about this file
        if self.files.contains_key(path_str) {
            return true; // Already recorded this file, nothing more to do.
        }

        // open the file and determine the size
        let fm_result = fs::metadata(path);
        if let Err(e) = fm_result {
            // Canonicalizing a missing file doesn't work, so
            // just use the current directory.
            let pbuf_result = PathBuf::from("./").canonicalize();
            let full_path = if let Ok(pbuf_result_unwrapped) = pbuf_result {
                // Hmm... seems like a lot of work to get the string
                pbuf_result_unwrapped.to_str().unwrap().to_string()
            } else {
                "!!Cannot determine full path!!".to_string()
            };
            let os_err = e.to_string();
            let m = format!(
                "Error getting metadata for file '{}'\n\
                    OS error is '{}'\n\
                    Looking in directory '{}'",
                path_str, os_err, full_path
            );
            diags.err1("IRDB_13", &m, path_opnd.src_loc.clone());
            return false;
        }

        let fm = fm_result.unwrap();

        if !fm.is_file() {
            let m = format!("'{}' must be a regular file.", path_str);
            diags.err1("IRDB_14", &m, path_opnd.src_loc.clone());
            return false;
        }

        let size = fm.len();

        let finfo = FileInfo {
            path: path_str.to_string(),
            size,
            src_loc: path_opnd.src_loc.clone(),
        };

        self.files.insert(path_str.to_string(), finfo);
        true
    }

    // Expect 1 operand which is an integer of some sort or bool
    fn validate_numeric_1(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 {
            let m = format!(
                "'{:?}' expressions must evaluate to one operand, but found {}.",
                ir.kind, len
            );
            diags.err1("IRDB_4", &m, ir.src_loc.clone());
            return false;
        }
        let opnd = &self.parms[ir.operands[0]];
        if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.val.data_type()) {
            let m = format!(
                "'{:?}' expression requires an integer or boolean operand, found '{:?}'.",
                ir.kind,
                opnd.val.data_type()
            );
            diags.err2("IRDB_5", &m, ir.src_loc.clone(), opnd.src_loc.clone());
            return false;
        }
        true
    }

    // Expect 2 operand which are int or bool
    fn validate_numeric_2(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 3 {
            let m = format!(
                "'{:?}' expression requires 2 input and one output \
                                    operands, but found {} total operands.",
                ir.kind, len
            );
            diags.err1("IRDB_6", &m, ir.src_loc.clone());
            return false;
        }
        for op_num in 0..2 {
            let opnd = &self.parms[ir.operands[op_num]];
            if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.val.data_type()) {
                let m = format!(
                    "'{:?}' expression requires an integer, found '{:?}'.",
                    ir.kind,
                    opnd.val.data_type()
                );
                diags.err2("IRDB_7", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                return false;
            }
        }
        true
    }

    // Expect 1 numeric operand (value) followed by one optional numeric operand (repeat count)
    fn validate_numeric_1_or_2(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 && len != 2 {
            let m = format!(
                "'{:?}' requires 1 or 2 input operands, \
                                  but found {} total operands.",
                ir.kind, len
            );
            diags.err1("IRDB_8", &m, ir.src_loc.clone());
            return false;
        }

        // First operand must be numeric
        let opnd = &self.parms[ir.operands[0]];
        if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.val.data_type()) {
            let m = format!(
                "'{:?}' requires an integer for this operand, \
                                    found '{:?}'.",
                ir.kind,
                opnd.val.data_type()
            );
            diags.err2("IRDB_9", &m, ir.src_loc.clone(), opnd.src_loc.clone());
            return false;
        }

        // Second *optional* operand must be numeric
        if len == 2 {
            let opnd = &self.parms[ir.operands[1]];
            if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.val.data_type()) {
                let m = format!(
                    "'{:?}' requires an integer for this operand, \
                                        found '{:?}'.",
                    ir.kind,
                    opnd.val.data_type()
                );
                diags.err2("IRDB_15", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                return false;
            }
        }
        true
    }

    fn validate_operands(
        &mut self,
        ir: &IR,
        diags: &mut Diags,
        ext_registry: &ExtensionRegistry,
        section_names: &HashSet<String>,
    ) -> bool {
        match ir.kind {
            IRKind::Align | IRKind::SetSecOffset | IRKind::SetAddrOffset | IRKind::SetAddr | IRKind::SetFileOffset | IRKind::Wr(_) => {
                self.validate_numeric_1_or_2(ir, diags)
            }
            IRKind::Assert => self.validate_numeric_1(ir, diags),
            IRKind::Wrf => self.validate_wrf_operands(ir, diags),
            IRKind::Wrs | IRKind::Print => self.validate_string_expr_operands(ir, diags),
            IRKind::Const => self.validate_const_operands(ir, diags),
            IRKind::ExtensionCall => {
                let name = self.get_opnd_as_identifier(ir, 0);
                if ext_registry.get(name).is_none() {
                    let m = if let Some(idx) = name.find("::") {
                        format!("Unknown namespace '{}'", &name[..idx])
                    } else {
                        format!("Unknown function '{}'", name)
                    };
                    diags.err1("IRDB_40", &m, ir.src_loc.clone());
                    return false;
                }
                true
            }
            IRKind::ExtensionCallRanged => {
                let name = self.get_opnd_as_identifier(ir, 0);
                if ext_registry.get(name).is_none() {
                    let m = if let Some(idx) = name.find("::") {
                        format!("Unknown namespace '{}'", &name[..idx])
                    } else {
                        format!("Unknown function '{}'", name)
                    };
                    diags.err1("IRDB_50", &m, ir.src_loc.clone());
                    return false;
                }
                // Need at least [name, start, length, output] = 4 operands.
                if ir.operands.len() < 4 {
                    let m = format!(
                        "Ranged extension '{}' requires (start_offset, length) \
                         as the first two arguments",
                        name
                    );
                    diags.err1("IRDB_45", &m, ir.src_loc.clone());
                    return false;
                }
                // operands[1] and operands[2] must be numeric.
                for opnd_pos in [1usize, 2usize] {
                    let opnd = &self.parms[ir.operands[opnd_pos]];
                    let dt = opnd.val.data_type();
                    if !matches!(dt, DataType::U64 | DataType::I64 | DataType::Integer) {
                        let m = format!(
                            "Ranged extension '{}': range argument {} must be a numeric \
                             expression, found {:?}",
                            name, opnd_pos, dt
                        );
                        diags.err2("IRDB_46", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                        return false;
                    }
                }
                true
            }
            IRKind::ExtensionCallSection => {
                let name = self.get_opnd_as_identifier(ir, 0);
                if ext_registry.get(name).is_none() {
                    let m = if let Some(idx) = name.find("::") {
                        format!("Unknown namespace '{}'", &name[..idx])
                    } else {
                        format!("Unknown function '{}'", name)
                    };
                    diags.err1("IRDB_51", &m, ir.src_loc.clone());
                    return false;
                }
                // Need at least [name, section_id, output] = 3 operands.
                assert!(
                    ir.operands.len() >= 3,
                    "ExtensionCallSection must have at least 3 operands"
                );
                // operands[1] must be an Identifier matching a known section.
                let sec_opnd = &self.parms[ir.operands[1]];
                let ParameterValue::Identifier(ref sec_name) = sec_opnd.val else {
                    let m = format!(
                        "Extension '{}': first argument must be a section identifier, \
                         found {:?}",
                        name,
                        sec_opnd.val.data_type()
                    );
                    diags.err2("IRDB_47", &m, ir.src_loc.clone(), sec_opnd.src_loc.clone());
                    return false;
                };
                if !section_names.contains(sec_name.as_str()) {
                    let m = format!(
                        "Extension '{}': unknown section '{}' in call",
                        name, sec_name
                    );
                    diags.err2("IRDB_48", &m, ir.src_loc.clone(), sec_opnd.src_loc.clone());
                    return false;
                }
                true
            }
            IRKind::SizeofExt => {
                let name = self.get_opnd_as_identifier(ir, 0);
                if ext_registry.get(name).is_none() {
                    let m = if let Some(idx) = name.find("::") {
                        format!("Unknown namespace '{}'", &name[..idx])
                    } else {
                        format!("Unknown extension '{}'", name)
                    };
                    diags.err1("IRDB_44", &m, ir.src_loc.clone());
                    return false;
                }
                true
            }
            IRKind::WrExt => {
                let len = ir.operands.len();
                if len != 1 {
                    let m = format!("'{:?}' requires exactly 1 input operand.", ir.kind);
                    diags.err1("IRDB_43", &m, ir.src_loc.clone());
                    return false;
                }
                let lop_idx = ir.operands[0];
                let lop = &self.parms[lop_idx];
                if lop.val.data_type() != DataType::Extension {
                    diags.err1(
                        "IRDB_41",
                        "Expected generic 'wr' statement to output an extension result.",
                        ir.src_loc.clone(),
                    );
                    return false;
                }
                true
            }
            IRKind::NEq
            | IRKind::LEq
            | IRKind::GEq
            | IRKind::Gt
            | IRKind::Lt
            | IRKind::DoubleEq
            | IRKind::LeftShift
            | IRKind::RightShift
            | IRKind::Multiply
            | IRKind::Divide
            | IRKind::Modulo
            | IRKind::BitAnd
            | IRKind::LogicalAnd
            | IRKind::BitOr
            | IRKind::LogicalOr
            | IRKind::Subtract
            | IRKind::Add => self.validate_numeric_2(ir, diags),
            IRKind::ToI64
            | IRKind::ToU64
            | IRKind::U64
            | IRKind::I64
            | IRKind::SectionStart
            | IRKind::SectionEnd
            | IRKind::Sizeof
            | IRKind::BuiltinOutputSize
            | IRKind::BuiltinOutputAddr
            | IRKind::BuiltinVersionString
            | IRKind::BuiltinVersionMajor
            | IRKind::BuiltinVersionMinor
            | IRKind::BuiltinVersionPatch
            | IRKind::Label
            | IRKind::Addr
            | IRKind::AddrOffset
            | IRKind::Eq
            | IRKind::SecOffset
            | IRKind::FileOffset
            // if/else and deferred-assignment IR — validated during sequential const walk
            | IRKind::ConstDeclare
            | IRKind::IfBegin
            | IRKind::ElseBegin
            | IRKind::IfEnd
            | IRKind::BareAssign => true,
        }
    }

    /// Convert the linear IR to real IR.  Conversion from Linear IR to real IR can fail,
    /// which is a hassle we don't want to deal with during linearization of the AST.
    fn process_linear_ir(
        &mut self,
        lin_db: &LayoutDb,
        diags: &mut Diags,
        ext_registry: &ExtensionRegistry,
    ) -> bool {
        // Section names come from LayoutDb, which collected them from ast_db.sections
        // at construction time.  This covers all declared sections, including non-output
        // sections that are never linearized into ir_vec.
        let section_names = &lin_db.section_names;

        let mut result = true;
        for lir in &lin_db.ir_vec {
            // Disambiguate ExtensionCall into the specific form before building the IR.
            // All extension calls arrive from LinearDB as IRKind::ExtensionCall; we
            // refine that here using the registry and section name set.
            let kind = if lir.op == IRKind::ExtensionCall {
                self.disambiguate_extension_call(lir, lin_db, ext_registry, section_names)
            } else {
                lir.op
            };

            let ir = IR {
                kind,
                operands: lir.operand_vec.clone(),
                src_loc: lir.src_loc.clone(),
            };
            let ir_num = self.ir_vec.len();
            if self.validate_operands(&ir, diags, ext_registry, section_names) {
                match kind {
                    IRKind::Label => {
                        // create the addressable entry and set the IR number
                        let name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        self.addressed_locs.insert(name, ir_num);
                    }
                    IRKind::SectionStart => {
                        // create the section entry and set the starting IR number
                        let sec_name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        let rng = Range {
                            start: ir_num,
                            end: 0,
                        };
                        self.sized_locs.insert(sec_name.clone(), rng);
                        self.addressed_locs.insert(sec_name, ir_num);
                    }
                    IRKind::SectionEnd => {
                        // Update the end of the range for this section
                        let sec_name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        let rng = self.sized_locs.get_mut(&sec_name).unwrap();
                        rng.end = ir_num;
                    }
                    _ => {}
                }
                self.ir_vec.push(ir);
            } else {
                result = false;
            }
        }
        result
    }

    /// Determines the specific IR kind for an ExtensionCall node.
    ///
    /// LinearDB emits `ExtensionCall` for all extension invocations.  This
    /// method inspects the first user argument and the registry to select the
    /// appropriate refined kind:
    ///
    /// - First user arg is an identifier matching a known section **and** the
    ///   extension is ranged → `ExtensionCallSection`
    /// - Extension is ranged (no section match) → `ExtensionCallRanged`
    /// - Extension is non-ranged → `ExtensionCall` (unchanged)
    /// - Extension not found → `ExtensionCall` (IRDB_40 will fire in validate_operands)
    fn disambiguate_extension_call(
        &self,
        lir: &LinIR,
        lin_db: &LayoutDb,
        ext_registry: &ExtensionRegistry,
        section_names: &HashSet<String>,
    ) -> IRKind {
        // operands layout: [name, user_arg0, ..., output]
        // At least 2 operands (name + output) always present; user args are in between.
        let has_user_args = lir.operand_vec.len() > 2;

        // Resolve the extension name from the name operand (always operands[0]).
        let name_op = &lin_db.operand_vec[lir.operand_vec[0]];
        let LinOperand::Literal {
            sval: name_sval, ..
        } = name_op
        else {
            panic!("Extension call name operand must be a Literal");
        };
        let Some(entry) = ext_registry.get(name_sval) else {
            return IRKind::ExtensionCall; // validate_operands will emit IRDB_40
        };

        if !entry.extension.is_ranged() {
            return IRKind::ExtensionCall;
        }

        // Ranged extension: check whether the first user arg is a section name.
        if has_user_args {
            let first_user_lop = &lin_db.operand_vec[lir.operand_vec[1]];
            if let LinOperand::Literal {
                tok: ast::LexToken::Identifier,
                sval,
                ..
            } = first_user_lop
                && section_names.contains(sval.as_str())
            {
                return IRKind::ExtensionCallSection;
            }
        }

        IRKind::ExtensionCallRanged
    }

    /// Resolve all const declarations in `const_db.const_map`, storing each
    /// resolved value in `self.symbol_table`.  Must be called before
    /// `process_lin_operands` so that const references can be substituted.
    /// Also walks `const_ir_vec` sequentially to process ConstDeclare and if/else IR.
    fn resolve_all_consts(&mut self, const_db: &ConstDb, diags: &mut Diags) -> bool {
        self.exec_const_statements(const_db, diags)
    }

    /// Sequential walk of `const_ir_vec` that handles all ConstDb IR kinds:
    /// `Const`, `ConstDeclare`, `IfBegin`, `ElseBegin`, `IfEnd`, `BareAssign`,
    /// and `Print`/`Assert` emitted inside if/else bodies.
    fn exec_const_statements(&mut self, const_db: &ConstDb, diags: &mut Diags) -> bool {
        /// Skip state for branches not taken.
        #[derive(Clone, Copy)]
        enum SkipState {
            /// Skip the then-body (condition was false); stop at ElseBegin (depth 0)
            /// or IfEnd (depth 0, meaning no else clause).
            SkipThen { depth: usize },
            /// Skip the else-body (condition was true); stop at IfEnd (depth 0).
            SkipElse { depth: usize },
        }

        let mut result = true;
        let mut skip_stack: Vec<SkipState> = Vec::new();

        let n = const_db.ir_vec.len();
        let mut idx = 0;
        while idx < n {
            let ir = &const_db.ir_vec[idx];
            let op = ir.op;
            let src_loc = ir.src_loc.clone();

            // If we're in a skip state, handle structural tokens to track depth.
            if let Some(&skip) = skip_stack.last() {
                match (skip, op) {
                    (SkipState::SkipThen { depth }, IRKind::IfBegin) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipThen { depth: depth + 1 };
                    }
                    (SkipState::SkipThen { depth: 0 }, IRKind::ElseBegin) => {
                        // Found the else of the if we're skipping — resume active processing.
                        skip_stack.pop();
                    }
                    (SkipState::SkipThen { depth }, IRKind::ElseBegin) => {
                        // Nested if's ElseBegin — no depth change (it's inside a nested if).
                        let _ = depth; // depth > 0, we're still skipping
                    }
                    (SkipState::SkipThen { depth: 0 }, IRKind::IfEnd) => {
                        // No else clause — resume active processing past IfEnd.
                        skip_stack.pop();
                    }
                    (SkipState::SkipThen { depth }, IRKind::IfEnd) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipThen { depth: depth - 1 };
                    }
                    (SkipState::SkipElse { depth }, IRKind::IfBegin) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipElse { depth: depth + 1 };
                    }
                    (SkipState::SkipElse { depth: 0 }, IRKind::IfEnd) => {
                        // Found the IfEnd matching the if whose else-body we're skipping.
                        skip_stack.pop();
                    }
                    (SkipState::SkipElse { depth }, IRKind::IfEnd) => {
                        *skip_stack.last_mut().unwrap() = SkipState::SkipElse { depth: depth - 1 };
                    }
                    _ => { /* any other IR inside a skipped block: ignore */ }
                }
                idx += 1;
                continue;
            }

            // Active processing.
            match op {
                IRKind::Const => {
                    let name_lop = &const_db.operand_vec[ir.operand_vec[0]];
                    let LinOperand::Literal { sval: name, .. } = name_lop else {
                        panic!("Const name operand must be a Literal");
                    };
                    let rhs_lop_num = ir.operand_vec[1];
                    let mut in_progress = HashSet::new();
                    let val = self.eval_const_expr_r(
                        rhs_lop_num,
                        const_db,
                        &mut in_progress,
                        diags,
                        &src_loc,
                    );
                    if let Some(v) = val {
                        if !self.symbol_table.contains_key(name) {
                            self.symbol_table
                                .define(name.to_string(), v, Some(src_loc.clone()));
                        }
                    } else {
                        result = false;
                    }
                }
                IRKind::ConstDeclare => {
                    let name_lop = &const_db.operand_vec[ir.operand_vec[0]];
                    let LinOperand::Literal { sval: name, .. } = name_lop else {
                        panic!("ConstDeclare name operand must be a Literal");
                    };
                    self.symbol_table.declare(name.clone(), src_loc);
                }
                IRKind::IfBegin => {
                    // Evaluate the condition operand.
                    let cond_lop_num = ir.operand_vec[0];
                    let mut in_progress = HashSet::new();
                    let cond_val = self.eval_const_expr_r(
                        cond_lop_num,
                        const_db,
                        &mut in_progress,
                        diags,
                        &src_loc,
                    );
                    match cond_val {
                        Some(v) if v.to_bool() => {
                            // Condition true: process then-body (no skip needed)
                        }
                        Some(_) => {
                            // Condition false: skip then-body
                            skip_stack.push(SkipState::SkipThen { depth: 0 });
                        }
                        None => {
                            result = false;
                            // Skip entire if/else to avoid cascading errors
                            skip_stack.push(SkipState::SkipThen { depth: 0 });
                        }
                    }
                }
                IRKind::ElseBegin => {
                    // Reached the else separator while in active then-body: skip else-body.
                    skip_stack.push(SkipState::SkipElse { depth: 0 });
                }
                IRKind::IfEnd => {
                    // End of an if/else we fully processed (no skip): nothing to do.
                }
                IRKind::BareAssign => {
                    let name_lop = &const_db.operand_vec[ir.operand_vec[0]];
                    let LinOperand::Literal { sval: name, .. } = name_lop else {
                        panic!("BareAssign name operand must be a Literal");
                    };
                    let name = name.clone();
                    let rhs_lop_num = ir.operand_vec[1];
                    let mut in_progress = HashSet::new();
                    let rhs_val = self.eval_const_expr_r(
                        rhs_lop_num,
                        const_db,
                        &mut in_progress,
                        diags,
                        &src_loc,
                    );
                    match rhs_val {
                        Some(v) => {
                            result &= self.symbol_table.assign(&name, v, &src_loc, diags);
                        }
                        None => {
                            result = false;
                        }
                    }
                }
                IRKind::Print => {
                    // Evaluate and print each operand as a string.
                    if !diags.noprint {
                        let mut s = String::new();
                        for &lop_idx in &ir.operand_vec {
                            let mut in_progress = HashSet::new();
                            match self.eval_const_expr_r(
                                lop_idx,
                                const_db,
                                &mut in_progress,
                                diags,
                                &src_loc,
                            ) {
                                Some(ParameterValue::QuotedString(ref v)) => s.push_str(v),
                                Some(ParameterValue::U64(v)) => s.push_str(&format!("{:#X}", v)),
                                Some(ParameterValue::I64(v) | ParameterValue::Integer(v)) => {
                                    s.push_str(&format!("{}", v));
                                }
                                Some(_) => {
                                    diags.err1(
                                        "IRDB_31",
                                        "Cannot print this value type in a const context",
                                        src_loc.clone(),
                                    );
                                    result = false;
                                }
                                None => {
                                    result = false;
                                }
                            }
                        }
                        if result {
                            print!("{}", s);
                        }
                    }
                }
                IRKind::Assert => {
                    let cond_lop_num = ir.operand_vec[0];
                    let mut in_progress = HashSet::new();
                    match self.eval_const_expr_r(
                        cond_lop_num,
                        const_db,
                        &mut in_progress,
                        diags,
                        &src_loc,
                    ) {
                        Some(v) if !v.to_bool() => {
                            diags.err1(
                                "IRDB_32",
                                "Assert expression failed in if/else body",
                                src_loc,
                            );
                            result = false;
                        }
                        None => {
                            result = false;
                        }
                        _ => {}
                    }
                }
                _ => { /* other IR kinds are not emitted into const_ir_vec */ }
            }
            idx += 1;
        }
        result
    }

    /// Evaluate a const expression operand recursively.
    /// Returns the computed `ParameterValue`, or `None` on error.
    fn eval_const_expr_r(
        &mut self,
        lop_num: usize,
        const_db: &ConstDb,
        _in_progress: &mut HashSet<String>,
        diags: &mut Diags,
        err_loc: &SourceSpan,
    ) -> Option<ParameterValue> {
        let lop = &const_db.operand_vec[lop_num];

        // Output operands: evaluate by looking up the producing instruction's IRKind.
        if let &LinOperand::Output { ir_lid, .. } = lop {
            let lin_ir = &const_db.ir_vec[ir_lid];
            let op = lin_ir.op;
            let op_loc = lin_ir.src_loc.clone();

            // Reject layout-time ops before evaluating any operands.
            match op {
                IRKind::Sizeof
                | IRKind::SizeofExt
                | IRKind::BuiltinOutputSize
                | IRKind::BuiltinOutputAddr
                | IRKind::Addr
                | IRKind::AddrOffset
                | IRKind::SecOffset
                | IRKind::FileOffset => {
                    let m = format!(
                        "Operation '{:?}' cannot be used in a const expression \
                         because it requires engine-time layout or addressing.",
                        op
                    );
                    diags.err1("IRDB_19", &m, op_loc);
                    return None;
                }
                _ => {}
            }

            // Version builtins are compile-time constants; resolve directly without operands.
            match op {
                IRKind::BuiltinVersionString => {
                    return Some(ParameterValue::QuotedString(
                        ConstBuiltins::get().brink_version_string.to_string(),
                    ));
                }
                IRKind::BuiltinVersionMajor => {
                    return Some(ParameterValue::U64(
                        ConstBuiltins::get().brink_version_major,
                    ));
                }
                IRKind::BuiltinVersionMinor => {
                    return Some(ParameterValue::U64(
                        ConstBuiltins::get().brink_version_minor,
                    ));
                }
                IRKind::BuiltinVersionPatch => {
                    return Some(ParameterValue::U64(
                        ConstBuiltins::get().brink_version_patch,
                    ));
                }
                _ => {}
            }

            // Binary, comparison, and logical ops: evaluate both input operands.
            let lhs_lop = lin_ir.operand_vec[0];
            let rhs_lop = lin_ir.operand_vec[1];
            let lhs_val =
                self.eval_const_expr_r(lhs_lop, const_db, _in_progress, diags, err_loc)?;
            let rhs_val =
                self.eval_const_expr_r(rhs_lop, const_db, _in_progress, diags, err_loc)?;
            return match op {
                IRKind::Add
                | IRKind::Subtract
                | IRKind::Multiply
                | IRKind::Divide
                | IRKind::Modulo
                | IRKind::BitAnd
                | IRKind::BitOr
                | IRKind::LeftShift
                | IRKind::RightShift => Self::apply_binary_op(op, lhs_val, rhs_val, &op_loc, diags),
                IRKind::DoubleEq
                | IRKind::NEq
                | IRKind::GEq
                | IRKind::LEq
                | IRKind::Gt
                | IRKind::Lt => Self::apply_comparison_op(op, lhs_val, rhs_val, &op_loc, diags),
                IRKind::LogicalAnd | IRKind::LogicalOr => {
                    let lhs_bool = lhs_val.to_bool();
                    let rhs_bool = rhs_val.to_bool();
                    let result = if op == IRKind::LogicalAnd {
                        lhs_bool && rhs_bool
                    } else {
                        lhs_bool || rhs_bool
                    };
                    Some(ParameterValue::U64(if result { 1 } else { 0 }))
                }
                _ => {
                    let m = format!(
                        "Operation '{:?}' is not supported in a const expression.",
                        op
                    );
                    diags.err1("IRDB_21", &m, err_loc.clone());
                    None
                }
            };
        }

        // Literal operands: evaluate directly from tok and sval.
        let LinOperand::Literal { tok, sval, src_loc } = lop else {
            unreachable!()
        };
        let sval = sval.clone();
        let src_loc = src_loc.clone();

        match tok {
            ast::LexToken::Integer => {
                let v: i64 = parse(&sval).ok().or_else(|| {
                    let m = format!("Malformed integer in const expression: {}", sval);
                    diags.err1("IRDB_22", &m, src_loc);
                    None
                })?;
                Some(ParameterValue::Integer(v))
            }
            ast::LexToken::U64 => {
                let s = sval.strip_suffix('u').unwrap_or(&sval).to_string();
                let v: u64 = parse(&s).ok().or_else(|| {
                    let m = format!("Malformed U64 in const expression: {}", sval);
                    diags.err1("IRDB_23", &m, src_loc);
                    None
                })?;
                Some(ParameterValue::U64(v))
            }
            ast::LexToken::I64 => {
                let s = sval.strip_suffix('i').unwrap_or(&sval).to_string();
                let v: i64 = parse(&s).ok().or_else(|| {
                    let m = format!("Malformed I64 in const expression: {}", sval);
                    diags.err1("IRDB_24", &m, src_loc);
                    None
                })?;
                Some(ParameterValue::I64(v))
            }
            ast::LexToken::QuotedString => {
                let trimmed = sval
                    .strip_prefix('"')
                    .unwrap_or(&sval)
                    .strip_suffix('"')
                    .unwrap_or(&sval)
                    .to_string();
                Some(ParameterValue::QuotedString(trimmed))
            }
            ast::LexToken::Identifier => {
                // Reference to another const.
                if let Some(val) = self.symbol_table.get_value(sval.as_str()) {
                    self.symbol_table.mark_used(sval.as_str());
                    Some(val)
                } else {
                    let m = format!(
                        "Unknown or uninitialized identifier '{}' in const expression. \
                         Constants must be defined before use.",
                        sval
                    );
                    diags.err1("IRDB_20", &m, src_loc);
                    None
                }
            }
            _ => {
                panic!(
                    "Literal operand with unexpected token {:?} in const expression",
                    tok
                );
            }
        }
    }

    /// Apply a binary arithmetic operator to two resolved const values.
    /// Promotes `Integer` to match a `U64` or `I64` operand when needed.
    fn apply_binary_op(
        op: IRKind,
        lhs: ParameterValue,
        rhs: ParameterValue,
        src_loc: &SourceSpan,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        use ParameterValue::*;
        // Reconcile Integer with a typed value; reject all other mismatches.
        let (lhs, rhs) = match (&lhs, &rhs) {
            (U64(_), U64(_))
            | (I64(_), I64(_))
            | (Integer(_), Integer(_))
            | (QuotedString(_), QuotedString(_)) => (lhs, rhs),
            (U64(_), Integer(v)) => (lhs, U64(*v as u64)),
            (Integer(v), U64(_)) => (U64(*v as u64), rhs),
            (I64(_), Integer(v)) => (lhs, I64(*v)),
            (Integer(v), I64(_)) => (I64(*v), rhs),
            _ => {
                let m = format!(
                    "Type mismatch in const expression: {:?} and {:?}.",
                    lhs.data_type(),
                    rhs.data_type()
                );
                diags.err1("IRDB_25", &m, src_loc.clone());
                return None;
            }
        };

        // Helper to emit the right diagnostic for a CalcErr and return None.
        let emit = |err: CalcErr, diags: &mut Diags| -> Option<ParameterValue> {
            match err {
                CalcErr::Overflow(msg) => {
                    diags.err1("IRDB_27", &msg, src_loc.clone());
                }
                CalcErr::DivByZero => {
                    diags.err1(
                        "IRDB_28",
                        "Division by zero in const expression",
                        src_loc.clone(),
                    );
                }
            }
            None
        };

        match lhs {
            U64(a) => {
                let b = rhs.to_u64();
                match Self::calc_u64_op(op, a, b) {
                    Ok(r) => Some(U64(r)),
                    Err(e) => emit(e, diags),
                }
            }
            I64(a) => {
                let b = rhs.to_i64();
                match Self::calc_i64_op(op, a, b) {
                    Ok(r) => Some(I64(r)),
                    Err(e) => emit(e, diags),
                }
            }
            Integer(a) => {
                let b = rhs.to_i64();
                match Self::calc_i64_op(op, a, b) {
                    Ok(r) => Some(Integer(r)),
                    Err(e) => emit(e, diags),
                }
            }
            _ => {
                let m = format!(
                    "Non-numeric type {:?} in arithmetic const expression.",
                    lhs.data_type()
                );
                diags.err1("IRDB_26", &m, src_loc.clone());
                None
            }
        }
    }

    /// Apply a comparison operator (==, !=, >=, <=) to two resolved const values.
    /// Returns U64(1) for true, U64(0) for false.
    /// Promotes `Integer` to match a `U64` or `I64` operand when needed.
    fn apply_comparison_op(
        op: IRKind,
        lhs: ParameterValue,
        rhs: ParameterValue,
        src_loc: &SourceSpan,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        use ParameterValue::*;
        // String equality/inequality: supported for == and != only.
        if let (QuotedString(a), QuotedString(b)) = (&lhs, &rhs) {
            let result = match op {
                IRKind::DoubleEq => a == b,
                IRKind::NEq => a != b,
                _ => {
                    let m = "Ordered comparison (>=, <=) is not supported for strings.".to_string();
                    diags.err1("IRDB_30", &m, src_loc.clone());
                    return None;
                }
            };
            return Some(U64(if result { 1 } else { 0 }));
        }
        // Reconcile Integer with a typed value; reject non-numeric types.
        let (lhs, rhs) = match (&lhs, &rhs) {
            (U64(_), U64(_)) | (I64(_), I64(_)) | (Integer(_), Integer(_)) => (lhs, rhs),
            (U64(_), Integer(v)) => (lhs, U64(*v as u64)),
            (Integer(v), U64(_)) => (U64(*v as u64), rhs),
            (I64(_), Integer(v)) => (lhs, I64(*v)),
            (Integer(v), I64(_)) => (I64(*v), rhs),
            _ => {
                let m = format!(
                    "Non-numeric or mismatched types in const comparison: {:?} and {:?}.",
                    lhs.data_type(),
                    rhs.data_type()
                );
                diags.err1("IRDB_29", &m, src_loc.clone());
                return None;
            }
        };

        let result = match lhs {
            U64(a) => {
                let b = rhs.to_u64();
                match op {
                    IRKind::DoubleEq => a == b,
                    IRKind::NEq => a != b,
                    IRKind::GEq => a >= b,
                    IRKind::LEq => a <= b,
                    IRKind::Gt => a > b,
                    IRKind::Lt => a < b,
                    _ => unreachable!(),
                }
            }
            I64(a) | Integer(a) => {
                let b = rhs.to_i64();
                match op {
                    IRKind::DoubleEq => a == b,
                    IRKind::NEq => a != b,
                    IRKind::GEq => a >= b,
                    IRKind::LEq => a <= b,
                    IRKind::Gt => a > b,
                    IRKind::Lt => a < b,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        };

        Some(U64(if result { 1 } else { 0 }))
    }

    fn calc_u64_op(op: IRKind, a: u64, b: u64) -> Result<u64, CalcErr> {
        match op {
            IRKind::Add => a.checked_add(b).ok_or_else(|| {
                CalcErr::Overflow(format!("Add expression '{a} + {b}' will overflow type U64"))
            }),
            IRKind::Subtract => a.checked_sub(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Subtract expression '{a} - {b}' will underflow type U64"
                ))
            }),
            IRKind::Multiply => a.checked_mul(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Multiply expression '{a} * {b}' will overflow type U64"
                ))
            }),
            IRKind::Divide => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a / b)
                }
            }
            IRKind::Modulo => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a % b)
                }
            }
            IRKind::BitAnd => Ok(a & b),
            IRKind::BitOr => Ok(a | b),
            IRKind::LeftShift => Ok(a << (b & 63)),
            IRKind::RightShift => Ok(a >> (b & 63)),
            _ => Err(CalcErr::Overflow(
                "Unknown operator in U64 const expression".to_string(),
            )),
        }
    }

    fn calc_i64_op(op: IRKind, a: i64, b: i64) -> Result<i64, CalcErr> {
        match op {
            IRKind::Add => a.checked_add(b).ok_or_else(|| {
                CalcErr::Overflow(format!("Add expression '{a} + {b}' will overflow type I64"))
            }),
            IRKind::Subtract => a.checked_sub(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Subtract expression '{a} - {b}' will underflow type I64"
                ))
            }),
            IRKind::Multiply => a.checked_mul(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Multiply expression '{a} * {b}' will overflow type I64"
                ))
            }),
            IRKind::Divide => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a / b)
                }
            }
            IRKind::Modulo => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a % b)
                }
            }
            IRKind::BitAnd => Ok(a & b),
            IRKind::BitOr => Ok(a | b),
            IRKind::LeftShift => Ok(a << (b & 63)),
            IRKind::RightShift => Ok(a >> (b & 63)),
            _ => Err(CalcErr::Overflow(
                "Unknown operator in I64 const expression".to_string(),
            )),
        }
    }

    pub fn new(
        const_db: &ConstDb,
        lin_db: &LayoutDb,
        diags: &mut Diags,
        defines: &HashMap<String, ParameterValue>,
        ext_registry: &ExtensionRegistry,
    ) -> anyhow::Result<Self> {
        let mut ir_db = IRDb {
            ir_vec: Vec::new(),
            parms: Vec::new(),
            sized_locs: HashMap::new(),
            addressed_locs: HashMap::new(),
            start_addr: 0,
            files: HashMap::new(),
            symbol_table: SymbolTable::new(),
            output_sec_str: lin_db.output_sec_str.clone(),
        };

        // Pre-populate the symbol table with command-line defines so they are
        // available to source const expressions and can override source consts.
        for (name, value) in defines {
            ir_db.symbol_table.define(name.clone(), value.clone(), None);
        }

        // Resolve all const declarations before anything else so their values
        // are available for substitution in operands and the output address.
        if !ir_db.resolve_all_consts(const_db, diags) {
            anyhow::bail!("IRDb construction failed.");
        }

        // Parse the optional output starting address.  If it is a const name,
        // look it up in the now-resolved symbol table.
        let start_addr = if let Some(addr_str) = lin_db.output_addr_str.as_ref() {
            if let Ok(addr) = parse::<u64>(addr_str) {
                addr
            } else if let Some(cv) = ir_db.symbol_table.get(addr_str.as_str()).cloned() {
                ir_db.symbol_table.mark_used(addr_str.as_str());
                cv.to_u64()
            } else {
                let m = format!("Malformed integer operand {}", addr_str);
                let loc = lin_db.output_addr_loc.as_ref().unwrap();
                diags.err1("IRDB_3", &m, loc.clone());
                anyhow::bail!("IRDb construction failed.");
            }
        } else {
            0
        };
        ir_db.start_addr = start_addr;

        if !ir_db.process_lin_operands(lin_db, diags) {
            anyhow::bail!("IRDb construction failed");
        }

        ir_db.symbol_table.warn_unused(diags);

        // To avoid panic, don't proceed into IR if the operands are bad.
        if !ir_db.process_linear_ir(lin_db, diags, ext_registry) {
            anyhow::bail!("IRDb construction failed");
        }

        Ok(ir_db)
    }

    pub fn dump(&self) {
        for (idx, ir) in self.ir_vec.iter().enumerate() {
            let mut op = format!("lid {}: is {:?}", idx, ir.kind);
            // display the operand for this LinIR
            let mut first = true;
            for child_idx in &ir.operands {
                let operand = &self.parms[*child_idx];
                if !first {
                    op.push(',');
                } else {
                    first = false;
                }
                if let Some(ir_lid) = operand.is_output_of() {
                    op.push_str(&format!(
                        " ({:?})tmp{}, output of lid {}",
                        operand.val.data_type(),
                        *child_idx,
                        ir_lid
                    ));
                } else {
                    match operand.val.data_type() {
                        DataType::U64 | DataType::Identifier => {
                            let v = operand.val.to_u64();
                            op.push_str(&format!(" ({:?}){:#X}", operand.val.data_type(), v));
                        }
                        DataType::I64 | DataType::Integer => {
                            let v = operand.val.to_i64();
                            op.push_str(&format!(" ({:?}){}", operand.val.data_type(), v));
                        }
                        DataType::QuotedString => {
                            let v = operand.val.to_str();
                            op.push_str(&format!(" ({:?}){}", operand.val.data_type(), v));
                        }
                        DataType::Extension => {
                            op.push_str(&format!(" ({:?})", operand.val.data_type()));
                        }
                        DataType::Unknown => {
                            println!("Dump: Found unknown Data Type operand {:?}", operand);
                            panic!();
                        }
                    }
                }
            }
            debug!("IRDb: {}", op);
        }
    }
}
