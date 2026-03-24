// Typed IR construction and validation for brink.
//
// IRDb is the third stage of the compiler pipeline.  It consumes the flat
// LinIR and LinOperand records from LinearDb and converts them into fully
// typed IR and IROperand values.  String operand values are parsed into their
// native Rust types (u64, i64, etc.) and each operand's DataType is resolved
// by recursively inspecting the expression tree.  IRDb also validates operand
// counts, type compatibility, and file-path operands (checking that referenced
// files exist and are readable), reporting any errors through Diags.
//
// Order of operations: irdb runs after lineardb.  Its output — an IRDb
// containing ir_vec, parms and file metadata — is consumed by engine.

use diags::Diags;
use diags::SourceSpan;
use lineardb::LinearDb;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

use ext::ExtensionRegistry;
use ir::{DataType, IR, IRKind, IROperand, ParameterValue};
use parse_int::parse;
use std::{
    collections::{HashMap, HashSet},
    fs,
    ops::Range,
    path::Path,
    path::PathBuf,
};

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

    /// Maps each const identifier to its resolved parameter value.
    pub const_values: HashMap<String, ParameterValue>,
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
        lin_db: &LinearDb,
        const_values: &HashMap<String, ParameterValue>,
        diags: &mut Diags,
    ) -> Option<DataType> {
        trace!(
            "IRDb::get_operand_data_type_r: Enter at depth {} for lop number {}",
            depth, lop_num
        );
        let lop = &lin_db.operand_vec[lop_num];
        let mut data_type = None;
        if let Some(lin_ir_lid) = lop.ir_lid
            && lin_db.ir_vec[lin_ir_lid].op == IRKind::ExtensionCall
        {
            return Some(DataType::Extension);
        }

        match lop.tok {
            // The following produce a boolean regardless of input data types
            ast::LexToken::Align
            | ast::LexToken::SetSec
            | ast::LexToken::SetImg
            | ast::LexToken::SetAbs
            | ast::LexToken::DoubleEq
            | ast::LexToken::NEq
            | ast::LexToken::GEq
            | ast::LexToken::LEq
            | ast::LexToken::Abs
            | ast::LexToken::Img
            | ast::LexToken::Sec
            | ast::LexToken::DoublePipe
            | ast::LexToken::DoubleAmpersand
            | ast::LexToken::Sizeof
            | ast::LexToken::ToU64
            | ast::LexToken::U64 => data_type = Some(DataType::U64), // TODO: this will be I64 when we convert bool
            ast::LexToken::ToI64 | ast::LexToken::I64 => data_type = Some(DataType::I64),
            ast::LexToken::Integer => data_type = Some(DataType::Integer),
            ast::LexToken::QuotedString => data_type = Some(DataType::QuotedString),
            ast::LexToken::Label => data_type = Some(DataType::Identifier),
            ast::LexToken::Namespace => data_type = Some(DataType::Identifier),
            ast::LexToken::Identifier => {
                // If this identifier is a resolved const, return the const's type.
                if let Some(cv) = const_values.get(lop.sval.as_str()) {
                    data_type = Some(cv.data_type());
                } else {
                    data_type = Some(DataType::Identifier);
                }
            }

            // The following produce an output type that depends on inputs
            ast::LexToken::DoubleLess
            | ast::LexToken::DoubleGreater
            | ast::LexToken::Pipe
            | ast::LexToken::Ampersand
            | ast::LexToken::Plus
            | ast::LexToken::Minus
            | ast::LexToken::Asterisk
            | ast::LexToken::Percent
            | ast::LexToken::FSlash => {
                // These operations have the same data type as their two inputs
                // The data type must be numeric.
                if lop.ir_lid.is_none() {
                    panic!("Output operand '{:?}' does not have a source lid", lop.tok);
                }

                let lin_ir_lid = lop.ir_lid.unwrap();
                let lin_ir = &lin_db.ir_vec[lin_ir_lid];
                // We expect 2 input and 1 output operand.
                assert!(lin_ir.operand_vec.len() == 3);
                // The lop this function was called with *is* the output operand
                assert!(lin_ir.operand_vec[2] == lop_num);
                let lhs_num = lin_ir.operand_vec[0];
                let rhs_num = lin_ir.operand_vec[1];

                let lhs_opt =
                    Self::get_operand_data_type_r(depth + 1, lhs_num, lin_db, const_values, diags);
                if let Some(lhs_dt) = lhs_opt {
                    let rhs_opt = Self::get_operand_data_type_r(
                        depth + 1,
                        rhs_num,
                        lin_db,
                        const_values,
                        diags,
                    );
                    if let Some(rhs_dt) = rhs_opt {
                        // We now have both lhs and rhs data types
                        if lhs_dt == rhs_dt {
                            let allowed = [DataType::I64, DataType::U64, DataType::Integer];
                            if !allowed.contains(&lhs_dt) {
                                let msg = format!(
                                    "Error, found data type '{:?}', but operation '{:?}' requires one of {:?}.",
                                    lhs_dt, lop.tok, allowed
                                );
                                diags.err1("IRDB_2", &msg, lin_ir.src_loc.clone());
                            } else {
                                data_type = Some(lhs_dt);
                            }
                        } else {
                            let mut dt_ok = false;
                            // Attempt to reconcile the data types
                            if rhs_dt == DataType::Integer {
                                if [DataType::I64, DataType::U64, DataType::Integer]
                                    .contains(&lhs_dt)
                                {
                                    dt_ok = true; // Integers work with s/u types
                                    data_type = Some(lhs_dt);
                                }
                            } else if lhs_dt == DataType::Integer
                                && [DataType::I64, DataType::U64].contains(&rhs_dt)
                            {
                                dt_ok = true; // Integers work with s/u types
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

            ast::LexToken::Wr8
            | ast::LexToken::Wr16
            | ast::LexToken::Wr24
            | ast::LexToken::Wr32
            | ast::LexToken::Wr40
            | ast::LexToken::Wr48
            | ast::LexToken::Wr56
            | ast::LexToken::Wr64
            | ast::LexToken::Assert
            | ast::LexToken::Const
            | ast::LexToken::Print
            | ast::LexToken::Section
            | ast::LexToken::OpenBrace
            | ast::LexToken::CloseBrace
            | ast::LexToken::Comma
            | ast::LexToken::OpenParen
            | ast::LexToken::CloseParen
            | ast::LexToken::Semicolon
            | ast::LexToken::Wrs
            | ast::LexToken::Wr
            | ast::LexToken::Wrf
            | ast::LexToken::Output
            | ast::LexToken::Eq
            | ast::LexToken::Unknown => {
                panic!("Token '{:?}' has no associated data type.", lop.tok);
            }
        };

        trace!(
            "IRDb::get_operand_data_type_r: Exit from depth {}, lop {} is {:?}",
            depth, lop_num, data_type
        );
        data_type
    }

    /// Process untyped linear operands into real IR operands
    fn process_lin_operands(&mut self, lin_db: &LinearDb, diags: &mut Diags) -> bool {
        trace!("IRDb::process_lin_operands: Enter");

        let mut result = true;
        for (lop_num, lop) in lin_db.operand_vec.iter().enumerate() {
            // If this identifier operand is a const reference, substitute the resolved
            // const value directly instead of keeping it as a bare Identifier.
            #[allow(clippy::collapsible_if)]
            if lop.tok == ast::LexToken::Identifier {
                if let Some(const_val) = self.const_values.get(lop.sval.as_str()).cloned() {
                    self.parms.push(IROperand {
                        ir_lid: None,
                        src_loc: lop.src_loc.clone(),
                        is_immediate: true,
                        val: const_val,
                    });
                    continue;
                }
            }

            let dt_opt =
                Self::get_operand_data_type_r(0, lop_num, lin_db, &self.const_values, diags);
            if dt_opt.is_none() {
                return false; // error case, just give up
            }

            let data_type = dt_opt.unwrap();

            // Determine if this operand is a constant value.  If so, operand construction
            // will convert the string representation to its native value.
            let is_immediate = lop.ir_lid.is_none();

            // During construction of the IROperand, the string in the linear operand is converted
            // to an actual typed value, which can fail, e.g. integer out of range
            let opnd = IROperand::new(
                lop.ir_lid,
                &lop.sval,
                &lop.src_loc,
                data_type,
                is_immediate,
                diags,
            );
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
    ) -> bool {
        match ir.kind {
            IRKind::Align | IRKind::SetSec | IRKind::SetImg | IRKind::SetAbs | IRKind::Wr(_) => {
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
            | IRKind::Label
            | IRKind::Abs
            | IRKind::Img
            | IRKind::Eq
            | IRKind::Sec => true,
        }
    }

    /// Convert the linear IR to real IR.  Conversion from Linear IR to real IR can fail,
    /// which is a hassle we don't want to deal with during linearization of the AST.
    fn process_linear_ir(
        &mut self,
        lin_db: &LinearDb,
        diags: &mut Diags,
        ext_registry: &ExtensionRegistry,
    ) -> bool {
        let mut result = true;
        for lir in &lin_db.ir_vec {
            let kind = lir.op;
            let ir = IR {
                kind,
                operands: lir.operand_vec.clone(),
                src_loc: lir.src_loc.clone(),
            };
            let ir_num = self.ir_vec.len();
            if self.validate_operands(&ir, diags, ext_registry) {
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

    /// Resolve all const declarations in `lin_db.const_map`, storing each
    /// resolved value in `self.const_values`.  Must be called before
    /// `process_lin_operands` so that const references can be substituted.
    fn resolve_all_consts(&mut self, lin_db: &LinearDb, diags: &mut Diags) -> bool {
        let names: Vec<String> = lin_db.const_map.keys().cloned().collect();
        let mut result = true;
        for name in names {
            let mut in_progress = HashSet::new();
            if self
                .resolve_const_by_name(&name, lin_db, &mut in_progress, diags)
                .is_none()
            {
                result = false;
            }
        }
        result
    }

    /// Resolve a single const by name, recursing into its dependencies.
    /// `in_progress` tracks names currently being evaluated to detect cycles.
    fn resolve_const_by_name(
        &mut self,
        name: &str,
        lin_db: &LinearDb,
        in_progress: &mut HashSet<String>,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        // Already resolved — return the cached value.
        if let Some(val) = self.const_values.get(name) {
            return Some(val.clone());
        }

        // Cycle detected.
        if in_progress.contains(name) {
            let ir_lid = lin_db.const_map[name];
            let src_loc = lin_db.const_ir_vec[ir_lid].src_loc.clone();
            let m = format!("Circular dependency detected for const '{}'", name);
            diags.err1("IRDB_18", &m, src_loc);
            return None;
        }

        let ir_lid = lin_db.const_map[name];
        let src_loc = lin_db.const_ir_vec[ir_lid].src_loc.clone();
        let rhs_lop_num = lin_db.const_ir_vec[ir_lid].operand_vec[1];

        in_progress.insert(name.to_string());
        let val = self.eval_lin_const_expr(rhs_lop_num, lin_db, in_progress, diags, &src_loc)?;
        in_progress.remove(name);

        self.const_values.insert(name.to_string(), val.clone());
        Some(val)
    }

    /// Evaluate a const expression operand recursively.
    /// Returns the computed `ParameterValue`, or `None` on error.
    fn eval_lin_const_expr(
        &mut self,
        lop_num: usize,
        lin_db: &LinearDb,
        in_progress: &mut HashSet<String>,
        diags: &mut Diags,
        err_loc: &SourceSpan,
    ) -> Option<ParameterValue> {
        let tok = lin_db.const_operand_vec[lop_num].tok;
        let sval = lin_db.const_operand_vec[lop_num].sval.clone();
        let src_loc = lin_db.const_operand_vec[lop_num].src_loc.clone();
        let ir_lid_opt = lin_db.const_operand_vec[lop_num].ir_lid;

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
                if lin_db.const_map.contains_key(sval.as_str()) {
                    self.resolve_const_by_name(&sval, lin_db, in_progress, diags)
                } else {
                    let m = format!(
                        "Unknown identifier '{}' in const expression.  \
                         Only const names may be referenced from const expressions.",
                        sval
                    );
                    diags.err1("IRDB_20", &m, src_loc);
                    None
                }
            }
            ast::LexToken::Sizeof
            | ast::LexToken::Abs
            | ast::LexToken::Img
            | ast::LexToken::Sec => {
                let m = format!(
                    "Operation '{:?}' cannot be used in a const expression \
                     because it requires engine-time layout or addressing.",
                    tok
                );
                diags.err1("IRDB_19", &m, src_loc);
                None
            }
            ast::LexToken::Plus
            | ast::LexToken::Minus
            | ast::LexToken::Asterisk
            | ast::LexToken::FSlash
            | ast::LexToken::Percent
            | ast::LexToken::Ampersand
            | ast::LexToken::Pipe
            | ast::LexToken::DoubleLess
            | ast::LexToken::DoubleGreater => {
                let ir_lid = ir_lid_opt.unwrap();
                let lhs_lop = lin_db.const_ir_vec[ir_lid].operand_vec[0];
                let rhs_lop = lin_db.const_ir_vec[ir_lid].operand_vec[1];
                let op_loc = lin_db.const_ir_vec[ir_lid].src_loc.clone();
                let lhs_val =
                    self.eval_lin_const_expr(lhs_lop, lin_db, in_progress, diags, err_loc)?;
                let rhs_val =
                    self.eval_lin_const_expr(rhs_lop, lin_db, in_progress, diags, err_loc)?;
                Self::apply_binary_op(tok, lhs_val, rhs_val, &op_loc, diags)
            }
            ast::LexToken::DoubleEq
            | ast::LexToken::NEq
            | ast::LexToken::GEq
            | ast::LexToken::LEq => {
                let ir_lid = ir_lid_opt.unwrap();
                let lhs_lop = lin_db.const_ir_vec[ir_lid].operand_vec[0];
                let rhs_lop = lin_db.const_ir_vec[ir_lid].operand_vec[1];
                let op_loc = lin_db.const_ir_vec[ir_lid].src_loc.clone();
                let lhs_val =
                    self.eval_lin_const_expr(lhs_lop, lin_db, in_progress, diags, err_loc)?;
                let rhs_val =
                    self.eval_lin_const_expr(rhs_lop, lin_db, in_progress, diags, err_loc)?;
                Self::apply_comparison_op(tok, lhs_val, rhs_val, &op_loc, diags)
            }
            _ => {
                let m = format!(
                    "Operation '{:?}' is not supported in a const expression.",
                    tok
                );
                diags.err1("IRDB_21", &m, err_loc.clone());
                None
            }
        }
    }

    /// Apply a binary arithmetic operator to two resolved const values.
    /// Promotes `Integer` to match a `U64` or `I64` operand when needed.
    fn apply_binary_op(
        tok: ast::LexToken,
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
                match Self::calc_u64_op(tok, a, b) {
                    Ok(r) => Some(U64(r)),
                    Err(e) => emit(e, diags),
                }
            }
            I64(a) => {
                let b = rhs.to_i64();
                match Self::calc_i64_op(tok, a, b) {
                    Ok(r) => Some(I64(r)),
                    Err(e) => emit(e, diags),
                }
            }
            Integer(a) => {
                let b = rhs.to_i64();
                match Self::calc_i64_op(tok, a, b) {
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
        tok: ast::LexToken,
        lhs: ParameterValue,
        rhs: ParameterValue,
        src_loc: &SourceSpan,
        diags: &mut Diags,
    ) -> Option<ParameterValue> {
        use ParameterValue::*;
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
                match tok {
                    ast::LexToken::DoubleEq => a == b,
                    ast::LexToken::NEq => a != b,
                    ast::LexToken::GEq => a >= b,
                    ast::LexToken::LEq => a <= b,
                    _ => unreachable!(),
                }
            }
            I64(a) | Integer(a) => {
                let b = rhs.to_i64();
                match tok {
                    ast::LexToken::DoubleEq => a == b,
                    ast::LexToken::NEq => a != b,
                    ast::LexToken::GEq => a >= b,
                    ast::LexToken::LEq => a <= b,
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        };

        Some(U64(if result { 1 } else { 0 }))
    }

    fn calc_u64_op(tok: ast::LexToken, a: u64, b: u64) -> Result<u64, CalcErr> {
        match tok {
            ast::LexToken::Plus => a.checked_add(b).ok_or_else(|| {
                CalcErr::Overflow(format!("Add expression '{a} + {b}' will overflow type U64"))
            }),
            ast::LexToken::Minus => a.checked_sub(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Subtract expression '{a} - {b}' will underflow type U64"
                ))
            }),
            ast::LexToken::Asterisk => a.checked_mul(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Multiply expression '{a} * {b}' will overflow type U64"
                ))
            }),
            ast::LexToken::FSlash => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a / b)
                }
            }
            ast::LexToken::Percent => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a % b)
                }
            }
            ast::LexToken::Ampersand => Ok(a & b),
            ast::LexToken::Pipe => Ok(a | b),
            ast::LexToken::DoubleLess => Ok(a << (b & 63)),
            ast::LexToken::DoubleGreater => Ok(a >> (b & 63)),
            _ => Err(CalcErr::Overflow(
                "Unknown operator in U64 const expression".to_string(),
            )),
        }
    }

    fn calc_i64_op(tok: ast::LexToken, a: i64, b: i64) -> Result<i64, CalcErr> {
        match tok {
            ast::LexToken::Plus => a.checked_add(b).ok_or_else(|| {
                CalcErr::Overflow(format!("Add expression '{a} + {b}' will overflow type I64"))
            }),
            ast::LexToken::Minus => a.checked_sub(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Subtract expression '{a} - {b}' will underflow type I64"
                ))
            }),
            ast::LexToken::Asterisk => a.checked_mul(b).ok_or_else(|| {
                CalcErr::Overflow(format!(
                    "Multiply expression '{a} * {b}' will overflow type I64"
                ))
            }),
            ast::LexToken::FSlash => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a / b)
                }
            }
            ast::LexToken::Percent => {
                if b == 0 {
                    Err(CalcErr::DivByZero)
                } else {
                    Ok(a % b)
                }
            }
            ast::LexToken::Ampersand => Ok(a & b),
            ast::LexToken::Pipe => Ok(a | b),
            ast::LexToken::DoubleLess => Ok(a << (b & 63)),
            ast::LexToken::DoubleGreater => Ok(a >> (b & 63)),
            _ => Err(CalcErr::Overflow(
                "Unknown operator in I64 const expression".to_string(),
            )),
        }
    }

    pub fn new(
        lin_db: &LinearDb,
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
            const_values: HashMap::new(),
        };

        // Pre-populate const_values with command-line defines so they are
        // available to source const expressions and can override source consts.
        for (name, value) in defines {
            ir_db.const_values.insert(name.clone(), value.clone());
        }

        // Resolve all const declarations before anything else so their values
        // are available for substitution in operands and the output address.
        if !ir_db.resolve_all_consts(lin_db, diags) {
            anyhow::bail!("IRDb construction failed.");
        }

        // Parse the optional output starting address.  If it is a const name,
        // look it up in the now-resolved const_values map.
        let start_addr = if let Some(addr_str) = lin_db.output_addr_str.as_ref() {
            if let Ok(addr) = parse::<u64>(addr_str) {
                addr
            } else if let Some(cv) = ir_db.const_values.get(addr_str.as_str()) {
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
