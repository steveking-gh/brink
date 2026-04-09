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

use diags::Diags;
use diags::SourceSpan;
use layoutdb::LayoutDb;
use linearizer::{LinIR, LinOperand};

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

    // Validate write file operands
    fn validate_wrf_operands(&mut self, ir: &IR, diags: &mut Diags) -> bool {
        // The parser always emits exactly one operand for wrf; a different count
        // indicates a linearizer bug, not a user error.
        assert!(ir.operands.len() == 1, "wrf must have exactly 1 operand");

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

    // Expect 1 numeric operand (value) followed by one optional numeric operand (repeat count)
    fn validate_numeric_1_or_2(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        // The linearizer always emits 1 or 2 operands for these instructions;
        // any other count indicates a linearizer bug, not a user error.
        assert!(len == 1 || len == 2, "{:?} must have 1 or 2 operands", ir.kind);

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
                // disambiguate_extension_call only produces ExtensionCallRanged for
                // extensions that exist in the registry; unknown names become
                // ExtensionCall and trigger IRDB_40 instead.
                assert!(ext_registry.get(name).is_some(), "ExtensionCallRanged with unknown extension '{name}'");
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
            | IRKind::Add => true, // operand count and types enforced by linearizer
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
            | IRKind::FileOffset => true,
            // These kinds are emitted only into ConstDb's internal IR vector and
            // are fully consumed during const evaluation.  They never enter the
            // layout IR vector that process_linear_ir iterates.
            IRKind::Const
            | IRKind::ConstDeclare
            | IRKind::IfBegin
            | IRKind::ElseBegin
            | IRKind::IfEnd
            | IRKind::BareAssign => unreachable!("const-only IR kind in layout IR"),
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

    pub fn new(
        symbol_table: SymbolTable,
        lin_db: &LayoutDb,
        diags: &mut Diags,
        ext_registry: &ExtensionRegistry,
    ) -> anyhow::Result<Self> {
        let mut ir_db = IRDb {
            ir_vec: Vec::new(),
            parms: Vec::new(),
            sized_locs: HashMap::new(),
            addressed_locs: HashMap::new(),
            start_addr: 0,
            files: HashMap::new(),
            symbol_table,
            output_sec_str: lin_db.output_sec_str.clone(),
        };

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

        // Warn about consts defined but never referenced by any operand.
        // Must run after process_lin_operands so all use-sites have called mark_used.
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
