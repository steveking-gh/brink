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

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

use extension_registry::{ExtensionRegistry, ParamKind};
use ir::{DataType, IR, IRKind, IROperand, ParameterValue, RegionBinding};
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

    /// Maps each bound section name to its region name (foreign key into region_bindings).
    /// Keyed by section name; consumed by LayoutPhase and later execution phases.
    pub section_region_names: HashMap<String, String>,

    /// All declared regions, keyed by region name.  Single source of truth for
    /// region addr/size; use region_for_section() to look up a section's binding.
    pub region_bindings: HashMap<String, RegionBinding>,
}

impl IRDb {
    /// Return the RegionBinding for a section, or None if the section is not
    /// bound to a region.
    pub fn region_for_section(&self, sec_name: &str) -> Option<&RegionBinding> {
        self.section_region_names.get(sec_name)
            .and_then(|rname| self.region_bindings.get(rname))
    }


    /// Returns the value of the specified operand for the specified IR.
    /// The operand number is for the *IR*, not the absolute operand
    /// index in the central operands vector.
    pub fn get_opnd_as_identifier(&self, ir: &IR, opnd_num: usize) -> &str {
        let &op_num = ir.operands.get(opnd_num).unwrap();
        let opnd = self.parms.get(op_num).unwrap();
        opnd.val.identifier_to_str()
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
                    // Extension call output: rejects use in arithmetic, wr8..64, wrs, const.
                    IRKind::ExtensionCall => return Some(DataType::Extension),

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

            linearizer::LinOperand::Literal { tok, .. } => {
                match tok {
                    ast::LexToken::U64 => data_type = Some(DataType::U64),
                    ast::LexToken::I64 => data_type = Some(DataType::I64),
                    ast::LexToken::Integer => data_type = Some(DataType::Integer),
                    ast::LexToken::QuotedString => data_type = Some(DataType::QuotedString),
                    ast::LexToken::Namespace => data_type = Some(DataType::Identifier),
                    ast::LexToken::BuiltinVersionString => data_type = Some(DataType::QuotedString),
                    _ => {
                        panic!("Literal operand with unexpected token {:?}", tok);
                    }
                }
            }

            // Identifier reference: type is the const's type if found, else Identifier.
            linearizer::LinOperand::Ref { sval, .. } => {
                if let Some(cv) = symbol_table.get(sval.as_str()) {
                    data_type = Some(cv.data_type());
                } else {
                    data_type = Some(DataType::Identifier);
                }
            }

            // NameDef: always resolves to Identifier.
            linearizer::LinOperand::NameDef { .. } => {
                data_type = Some(DataType::Identifier);
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
            // Const substitution: replace Ref operands that name a resolved const
            // with the const's typed value.
            if let linearizer::LinOperand::Ref { sval, src_loc, param_name } = lop
                && let Some(const_val) = self.symbol_table.get(sval.as_str()).cloned()
            {
                self.symbol_table.mark_used(sval.as_str());
                self.parms.push(IROperand {
                    ir_lid: None,
                    src_loc: src_loc.clone(),
                    is_immediate: true,
                    val: const_val,
                    param_name: param_name.clone(),
                });
                continue;
            }

            let dt_opt =
                Self::get_operand_data_type_r(0, lop_num, lin_db, &self.symbol_table, diags);
            let Some(data_type) = dt_opt else {
                return false; // error case, just give up
            };

            // Destructure fields needed by IROperand::new.  Output operands carry no
            // sval (the engine initializes their value at execution time), so pass "".
            let (ir_lid, sval, src_loc, is_immediate, param_name) = match lop {
                linearizer::LinOperand::Literal { sval, src_loc, param_name, .. } => {
                    (None, sval.as_str(), src_loc, true, param_name.clone())
                }
                linearizer::LinOperand::Ref { sval, src_loc, param_name } => {
                    (None, sval.as_str(), src_loc, true, param_name.clone())
                }
                linearizer::LinOperand::NameDef { sval, src_loc } => {
                    (None, sval.as_str(), src_loc, true, None)
                }
                linearizer::LinOperand::Output { ir_lid, src_loc, param_name } => {
                    (Some(*ir_lid), "", src_loc, false, param_name.clone())
                }
            };

            // Convert the string literal to a typed value; fails on malformed input.
            let opnd = IROperand::new(ir_lid, sval, src_loc, data_type, is_immediate, diags);
            if let Some(mut opnd) = opnd {
                opnd.param_name = param_name;
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
        if !(1..=2).contains(&len) {
            let m = format!(
                "'{:?}' takes 1 or 2 arguments, found {}.",
                ir.kind, len
            );
            diags.err1("IRDB_55", &m, ir.src_loc.clone());
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

    /// Validates that every extension user argument in the given operand
    /// index range is a value type (numeric, string, or section-name identifier).
    /// Extension outputs and unknown types are rejected with IRDB_47 because
    /// the engine cannot convert them to ParamArg.
    fn validate_ext_user_args(
        &self,
        ir: &IR,
        user_arg_range: std::ops::Range<usize>,
        ext_name: &str,
        diags: &Diags,
    ) -> bool {
        for opnd_pos in user_arg_range {
            let opnd = &self.parms[ir.operands[opnd_pos]];
            let dt = opnd.val.data_type();
            if !matches!(
                dt,
                DataType::U64
                | DataType::I64
                | DataType::Integer
                | DataType::QuotedString
                // Identifier section names resolve to ParamArg::Slice at engine time.
                | DataType::Identifier
            ) {
                let m = format!(
                    "Extension '{}': argument {} must be a numeric, string, or section-name expression, got {:?}",
                    ext_name, opnd_pos, dt
                );
                diags.err2("IRDB_47", &m, ir.src_loc.clone(), opnd.src_loc.clone());
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
        _section_names: &HashSet<String>,
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
                // User args: operands[1..last] (operands[0]=name, operands[last]=output).
                let last = ir.operands.len() - 1;
                self.validate_ext_user_args(ir, 1..last, name, diags)
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

    /// Emits IRDB_52: a Slice parameter received a value expression instead of a section name.
    /// Centralizes the error code so the uniqueness check finds exactly one occurrence.
    fn err_bytearray_needs_section(
        ir: &IR,
        param_name: &str,
        opnd: &IROperand,
        ext_name: &str,
        diags: &mut Diags,
    ) {
        let m = format!(
            "Extension '{}': parameter '{}' has kind Slice and requires a section name, \
             not a value expression",
            ext_name, param_name
        );
        diags.err2("IRDB_52", &m, ir.src_loc.clone(), opnd.src_loc.clone());
    }

    /// Validates and canonicalizes named/positional arguments for one ExtensionCall IR.
    ///
    /// When the extension declares params() and the call site uses named args:
    ///   - Validates every arg name against params() (IRDB_48).
    ///   - Rejects duplicate names (IRDB_49).
    ///   - Rejects missing required params (IRDB_51).
    ///   - Rejects Slice params that received a non-Identifier value (IRDB_52).
    ///   - Reorders ir.operands[1..last] to declaration order.
    ///
    /// When the extension declares params() and the call site uses positional args:
    ///   - Validates argument count matches params() length (IRDB_53).
    ///
    /// When the extension returns an empty params() slice (legacy opt-out):
    ///   - No validation beyond what validate_operands already performs.
    ///   - The engine applies the first-Identifier-is-section heuristic.
    fn resolve_named_ext_args(
        &self,
        ir: &mut IR,
        ext_registry: &ExtensionRegistry,
        section_names: &HashSet<String>,
        diags: &mut Diags,
    ) -> bool {
        let name = self.get_opnd_as_identifier(ir, 0);
        let Some(entry) = ext_registry.get(name) else {
            return true; // Unknown extension; validate_operands fires IRDB_40.
        };
        let params = &entry.cached_params;

        // operands layout: [name, user_arg0..., output]
        // last = index of the trailing output operand
        let last = ir.operands.len() - 1;
        let user_count = last - 1; // operands[1..last]

        if params.is_empty() {
            // Legacy path: no named-arg enforcement.  Engine heuristic applies.
            return true;
        }

        // Detect whether any user arg carries a param_name (named-args mode).
        let any_named = (1..last).any(|i| self.parms[ir.operands[i]].param_name.is_some());

        if any_named {
            // Named-args mode: resolve, validate, and reorder to declaration order.
            let mut name_to_opnd_idx: HashMap<&str, usize> = HashMap::new();
            for i in 1..last {
                let opnd_idx = ir.operands[i];
                let opnd = &self.parms[opnd_idx];
                let param_name = match &opnd.param_name {
                    Some(n) => n.as_str(),
                    None => continue, // AST_40 would have caught mixing; skip stray positional.
                };
                // IRDB_48: unknown param name.
                if !params.iter().any(|p| p.name == param_name) {
                    let m = format!(
                        "Extension '{}': unknown parameter name '{}'",
                        name, param_name
                    );
                    diags.err2("IRDB_48", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                    return false;
                }
                // IRDB_49: duplicate name.
                if name_to_opnd_idx.contains_key(param_name) {
                    let m = format!(
                        "Extension '{}': duplicate parameter name '{}'",
                        name, param_name
                    );
                    diags.err2("IRDB_49", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                    return false;
                }
                name_to_opnd_idx.insert(param_name, opnd_idx);
            }

            // IRDB_51: every declared param must be present.
            for p in params.iter() {
                if !name_to_opnd_idx.contains_key(p.name) {
                    let m = format!(
                        "Extension '{}': missing required parameter '{}'",
                        name, p.name
                    );
                    diags.err1("IRDB_51", &m, ir.src_loc.clone());
                    return false;
                }
            }

            // IRDB_52: Slice params must receive an Identifier (section name).
            for p in params.iter() {
                if p.kind == ParamKind::Slice {
                    let opnd_idx = name_to_opnd_idx[p.name];
                    let opnd = &self.parms[opnd_idx];
                    if !matches!(opnd.val, ParameterValue::Identifier(_)) {
                        Self::err_bytearray_needs_section(ir, p.name, opnd, name, diags);
                        return false;
                    }
                }
            }

            // Reorder user-arg operand slots to declaration order.
            for (i, p) in params.iter().enumerate() {
                ir.operands[1 + i] = name_to_opnd_idx[p.name];
            }
        } else {
            // Positional mode with params declared: validate count (IRDB_53).
            if user_count != params.len() {
                let m = format!(
                    "Extension '{}': expected {} argument(s), got {}",
                    name,
                    params.len(),
                    user_count
                );
                diags.err1("IRDB_53", &m, ir.src_loc.clone());
                return false;
            }
            // Validate Slice positional params received an Identifier (section name).
            for (i, p) in params.iter().enumerate() {
                if p.kind == ParamKind::Slice {
                    let opnd = &self.parms[ir.operands[1 + i]];
                    if !matches!(opnd.val, ParameterValue::Identifier(_)) {
                        // IRDB_52: value expression where a section name is required.
                        Self::err_bytearray_needs_section(ir, p.name, opnd, name, diags);
                        return false;
                    }
                    // IRDB_54: the identifier does not name a known section.
                    let ParameterValue::Identifier(ref sec_name) = opnd.val else { unreachable!() };
                    if !section_names.contains(sec_name.as_str()) {
                        let m = format!(
                            "Extension '{}': parameter '{}' names an unknown section '{}'",
                            name, p.name, sec_name
                        );
                        diags.err2("IRDB_54", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                        return false;
                    }
                }
            }
        }

        true
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
            let mut ir = IR {
                kind: lir.op,
                operands: lir.operand_vec.clone(),
                src_loc: lir.src_loc.clone(),
            };

            // For extension calls, resolve named args and reorder operands to declaration
            // order before validation.  Positional calls with params() declared also have
            // their argument count checked here.
            if ir.kind == IRKind::ExtensionCall
                && !self.resolve_named_ext_args(&mut ir, ext_registry, section_names, diags)
            {
                result = false;
                continue;
            }

            let ir_num = self.ir_vec.len();
            if self.validate_operands(&ir, diags, ext_registry, section_names) {
                match ir.kind {
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

    pub fn new(
        symbol_table: SymbolTable,
        lin_db: &LayoutDb,
        diags: &mut Diags,
        ext_registry: &ExtensionRegistry,
        section_region_names: HashMap<String, String>,
        region_bindings: HashMap<String, RegionBinding>,
    ) -> anyhow::Result<Self> {
        let mut ir_db = IRDb {
            ir_vec: Vec::new(),
            parms: Vec::new(),
            sized_locs: HashMap::new(),
            addressed_locs: HashMap::new(),
            files: HashMap::new(),
            symbol_table,
            output_sec_str: lin_db.output_sec_str.clone(),
            section_region_names,
            region_bindings,
        };

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
                        DataType::U64 => {
                            // Display U64 as hex since that's generally most helpful.
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
                        DataType::Identifier => {
                            let v = operand.val.identifier_to_str();
                            op.push_str(&format!(" ({:?}){}", operand.val.data_type(), v));
                        }
                        DataType::Extension => {
                            op.push_str(&format!("({:?}){}", operand.val.data_type(), operand.val.to_str()));
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
