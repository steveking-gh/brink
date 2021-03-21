pub type Span = std::ops::Range<usize>;
use diags::Diags;
use lineardb::{LinearDb};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ir::{DataType, IR, IRKind, IROperand};
use std::{collections::HashMap, ops::Range};
use parse_int::parse;

pub struct IRDb {
    pub ir_vec: Vec<IR>,
    pub parms: Vec<IROperand>,

    /// The optional absolute starting address specified
    /// in the output statement.  Zero by default.
    pub start_addr: u64,

    /// Maps an identifier to the (start,stop) indices in the ir_vec.
    /// Used for items with a size (potentially zero) such as sections.
    pub sized_locs: HashMap<String,Range<usize>>,

    /// Maps an identifier to the start indices in the ir_vec.
    /// Used for items that are addressable, including sections and labels
    pub addressed_locs: HashMap<String,usize>,
}

impl IRDb {

    /// Returns the value of the specified operand for the specified IR.
    /// The operand number is for the *IR*, not the absolute operand
    /// index in the central operands vector.
    pub fn get_opnd_as_identifier(&self, ir: &IR, opnd_num: usize) -> &str {
        let &op_num = ir.operands.get(opnd_num).unwrap();
        let opnd = self.parms.get(op_num).unwrap();
        opnd.to_identifier()
    }

    pub fn get_operand_ir_lid(&self, opnd_num: usize) -> Option<usize> {
        self.parms.get(opnd_num).unwrap().ir_lid
    }

    /// Get the datatype of the referenced operand by recursively inspecting
    /// the input operands.
    /// Returns None on error
    fn get_operand_data_type_r(&mut self, depth: usize, lop_num: usize, lin_db: &LinearDb,
                                diags: &mut Diags) -> Option<DataType> {
        trace!("IRDb::get_operand_data_type_r: Enter at depth {} for lop number {}", depth, lop_num);
        let lop = &lin_db.operand_vec[lop_num];
        let mut data_type = None;
        
        match lop.tok {
            // The following produce a boolean regardless of input data types
            ast::LexToken::DoubleEq |
            ast::LexToken::NEq |
            ast::LexToken::GEq |
            ast::LexToken::LEq |
            ast::LexToken::Abs |
            ast::LexToken::Img |
            ast::LexToken::Sec |
            ast::LexToken::DoublePipe |
            ast::LexToken::DoubleAmpersand |
            ast::LexToken::Sizeof |
            ast::LexToken::ToU64 |
            ast::LexToken::U64 => { data_type = Some(DataType::U64) } // TODO: this will be I64 when we convert bool
            ast::LexToken::ToI64 |
            ast::LexToken::I64 => { data_type = Some(DataType::I64) }
            ast::LexToken::Integer => { data_type = Some(DataType::Integer) }
            ast::LexToken::QuotedString => { data_type = Some(DataType::QuotedString) }
            ast::LexToken::Label => { data_type = Some(DataType::Identifier) }
            ast::LexToken::Identifier => { data_type = Some(DataType::Identifier) }
            
            // The following produce an output type that depends on inputs
            ast::LexToken::DoubleLess |
            ast::LexToken::DoubleGreater |
            ast::LexToken::Pipe |
            ast::LexToken::Ampersand |
            ast::LexToken::Plus |
            ast::LexToken::Minus |
            ast::LexToken::Asterisk |
            ast::LexToken::Percent |
            ast::LexToken::FSlash => {
                // These operations have the same data type as their two inputs
                // The data type must be numeric.
                if lop.ir_lid.is_none() {
                    panic!("Output operand '{:?}' does not have a source lid", lop.tok);
                }

                let lin_ir_lid = lop.ir_lid.unwrap();
                let lin_ir = &lin_db.ir_vec[lin_ir_lid];
                // We expect 2 input and 1 output operand.
                assert!(lin_ir.operand_vec.len() == 3);
                // The lop this this function was called with *is* the output operand
                assert!(lin_ir.operand_vec[2] == lop_num);
                let lhs_num = lin_ir.operand_vec[0];
                let rhs_num = lin_ir.operand_vec[1];
                
                let lhs_opt = self.get_operand_data_type_r(depth + 1, lhs_num, lin_db, diags);
                if let Some(lhs_dt) = lhs_opt {
                    let rhs_opt = self.get_operand_data_type_r(depth + 1, rhs_num, lin_db, diags);
                    if let Some(rhs_dt) = rhs_opt {
                        // We now have both lhs and rhs data types
                        if lhs_dt == rhs_dt {
                            let allowed = [DataType::I64, DataType::U64, DataType::Integer];
                            if !allowed.contains(&lhs_dt) {
                                let msg = format!("Error, found data type '{:?}', but operation '{:?}' requires one of {:?}.",
                                                lhs_dt, lop.tok, allowed);
                                diags.err1("IRDB_2", &msg, lin_ir.src_loc.clone());
                            } else {
                                data_type = Some(lhs_dt);
                            }
                        } else {
                            let mut dt_ok = false;
                            // Attempt to reconcile the data types
                            if rhs_dt == DataType::Integer {
                                if [DataType::I64, DataType::U64, DataType::Integer].contains(&lhs_dt) {
                                    dt_ok = true; // Integers work with s/u types
                                    data_type = Some(lhs_dt);
                                }
                            } else if lhs_dt == DataType::Integer {
                                if [DataType::I64, DataType::U64].contains(&rhs_dt) {
                                    dt_ok = true; // Integers work with s/u types
                                    data_type = Some(rhs_dt);
                                }
                            }
                
                            if !dt_ok {
                                let msg = format!("Error, data type mismatch in input operands.  Left is {:?}, right is {:?}.",
                                lhs_dt, rhs_dt);
                                diags.err1("IRDB_1", &msg, lin_ir.src_loc.clone());
                            }
                        }
                    }
                }
            }
            ast::LexToken::Wr8  |
            ast::LexToken::Wr16 |
            ast::LexToken::Wr24 |
            ast::LexToken::Wr32 |
            ast::LexToken::Wr40 |
            ast::LexToken::Wr48 |
            ast::LexToken::Wr56 |
            ast::LexToken::Wr64 |
            ast::LexToken::Assert |
            ast::LexToken::Print |
            ast::LexToken::Section |
            ast::LexToken::OpenBrace |
            ast::LexToken::CloseBrace |
            ast::LexToken::Comma |
            ast::LexToken::OpenParen |
            ast::LexToken::CloseParen |
            ast::LexToken::Semicolon |
            ast::LexToken::Wrs |
            ast::LexToken::Wr |
            ast::LexToken::Output |
            ast::LexToken::Unknown => { panic!("Token '{:?}' has no associated data type.", lop.tok); }
        };

        trace!("IRDb::get_operand_data_type_r: Exit from depth {}, lop {} is {:?}", depth, lop_num, data_type);
        data_type
    }

    /// Process untyped linear operands into real IR operands
    fn process_lin_operands(&mut self, lin_db: &LinearDb, diags: &mut Diags) -> bool {
        trace!("IRDb::process_lin_operands: Enter");

        let mut result = true;
        let len = lin_db.operand_vec.len();
        for lop_num in 0..len {
            let dt_opt = self.get_operand_data_type_r(0, lop_num, lin_db, diags);
            if dt_opt.is_none() {
                return false; // error case, just give up
            }

            let data_type = dt_opt.unwrap();
            let lop = &lin_db.operand_vec[lop_num];

            // Determine if this operand is a constant value.  If so, operand construction
            // will convert the string representation to its native value.
            let is_constant = lop.ir_lid.is_none();

            // During construction of the IROperand, the string in the linear operand is converted
            // to an actual typed value, which can fail, e.g. integer out of range
            let opnd = IROperand::new( lop.ir_lid, &lop.sval, &lop.src_loc, data_type,
                                                    is_constant, diags);
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

    // Expect 1 operand which is an integer of some sort or bool
    fn validate_numeric_operand(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 {
            let m = format!("'{:?}' expressions must evaluate to one operand, but found {}.", ir.kind, len);
            diags.err1("IRDB_4", &m, ir.src_loc.clone());
            return false;
        }
        let opnd = &self.parms[ir.operands[0]];
        if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.data_type) {
            let m = format!("'{:?}' expression requires an integer or boolean operand, found '{:?}'.", ir.kind, opnd.data_type);
            diags.err2("IRDB_5", &m, ir.src_loc.clone(), opnd.src_loc.clone());
            return false;
        }
        true
    }

    // Expect 2 operand which are int or bool
    fn validate_numeric_operands2(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 3 {
            let m = format!("'{:?}' expression requires 2 input and one output \
                                    operands, but found {} total operands.", ir.kind, len);
            diags.err1("IRDB_6", &m, ir.src_loc.clone());
            return false;
        }
        for op_num in 0..2 {
            let opnd = &self.parms[ir.operands[op_num]];
            if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.data_type) {
                let m = format!("'{:?}' expression requires an integer, found '{:?}'.",
                                    ir.kind, opnd.data_type);
                diags.err2("IRDB_7", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                return false;
            }
        }
        true
    }

    // Expect 1 numeric operand (value) followed by one optional numeric operand (repeat count)
    fn validate_wrx_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 && len != 2 {
            let m = format!("'{:?}' requires 1 or 2 input operands, \
                                  but found {} total operands.", ir.kind, len);
            diags.err1("IRDB_8", &m, ir.src_loc.clone());
            return false;
        }

        // First operand must be numeric
        let opnd = &self.parms[ir.operands[0]];
        if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.data_type) {
            let m = format!("'{:?}' requires an integer for this operand, \
                                    found '{:?}'.", ir.kind, opnd.data_type);
            diags.err2("IRDB_9", &m, ir.src_loc.clone(), opnd.src_loc.clone());
            return false;
        }

        // Second *optional* operand must be numeric
        if len == 2 {
            let opnd = &self.parms[ir.operands[1]];
            if ![DataType::Integer, DataType::I64, DataType::U64].contains(&opnd.data_type) {
                let m = format!("'{:?}' requires an integer for this operand, \
                                        found '{:?}'.", ir.kind, opnd.data_type);
                diags.err2("IRDB_9", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                return false;
            }
        }
        true
    }

    fn validate_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let result = match ir.kind {
            IRKind::Wr8 |
            IRKind::Wr16 |
            IRKind::Wr24 |
            IRKind::Wr32 |
            IRKind::Wr40 |
            IRKind::Wr48 |
            IRKind::Wr56 |
            IRKind::Wr64 => { self.validate_wrx_operands(ir, diags) }
            IRKind::Assert => { self.validate_numeric_operand(ir, diags) }
            IRKind::Wrs |
            IRKind::Print => { self.validate_string_expr_operands(ir, diags) }
            IRKind::NEq |
            IRKind::LEq |
            IRKind::GEq |
            IRKind::DoubleEq |
            IRKind::LeftShift |
            IRKind::RightShift |
            IRKind::Multiply |
            IRKind::Divide |
            IRKind::Modulo |
            IRKind::BitAnd |
            IRKind::LogicalAnd |
            IRKind::BitOr |
            IRKind::LogicalOr |
            IRKind::Subtract |
            IRKind::Add => { self.validate_numeric_operands2(ir, diags) }
            IRKind::ToI64 |
            IRKind::ToU64 |
            IRKind::U64 |
            IRKind::I64 |
            IRKind::SectionStart |
            IRKind::SectionEnd |
            IRKind::Sizeof |
            IRKind::Label |
            IRKind::Abs |
            IRKind::Img |
            IRKind::Sec => { true }
        };
        result
    }

    /// Convert the linear IR to real IR.  Conversion from Linear IR to real IR can fail,
    /// which is a hassle we don't want to deal with during linearization of the AST.
    fn process_linear_ir(&mut self, lin_db: &LinearDb, diags: &mut Diags) -> bool {
        let mut result = true;
        for lir in &lin_db.ir_vec {
            let kind = lir.op;
            // The operands are just indices into the operands array
            let operands = lir.operand_vec.clone();
            let src_loc = lir.src_loc.clone();
            let ir = IR{kind, operands, src_loc};
            let ir_num = self.ir_vec.len();
            if self.validate_operands(&ir, diags) {
                match kind {
                    IRKind::Label => {
                        // create the addressable entry and set the IR number
                        let name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        self.addressed_locs.insert(name, ir_num);
                    }
                    IRKind::SectionStart => {
                        // create the section entry and set the starting IR number
                        let sec_name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        let rng = Range {start: ir_num, end: 0};
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

    pub fn new(lin_db: &LinearDb, diags: &mut Diags) -> Option<IRDb> {

        // If the user specified a starting address in the output statement
        // then convert to a real number
        let mut start_addr = 0;

        if let Some(addr_str) = lin_db.output_addr_str.as_ref() {
            if let Ok(addr) = parse::<u64>(addr_str) {
                start_addr = addr;
            } else {
                let m = format!("Malformed integer operand {}", addr_str);
                let primary_code_ref = lin_db.output_addr_loc.as_ref().unwrap();
                diags.err1("IRDB_3", &m, primary_code_ref.clone());
                return None;                
            }
        }

        let mut ir_db = IRDb { ir_vec: Vec::new(), parms: Vec::new(),
            sized_locs: HashMap::new(), addressed_locs: HashMap::new(), start_addr };

        if !ir_db.process_lin_operands(lin_db, diags) {
            return None;
        }

        // To avoid panic, don't proceed into IR if the operands are bad.
        if !ir_db.process_linear_ir(lin_db, diags) {
            return None;
        }
        
        Some(ir_db)
    }

    pub fn dump(&self) {
        for (idx,ir) in self.ir_vec.iter().enumerate() {
            let mut op = format!("lid {}: is {:?}", idx, ir.kind);
            // display the operand for this LinIR
            let mut first = true;
            for child in &ir.operands {
                let operand = &self.parms[*child];
                if !first {
                    op.push_str(",");
                } else {
                    first = false;
                }
                if let Some(ir_lid) = operand.is_output_of() {
                    op.push_str(&format!(" ({:?})tmp{}, output of lid {}", operand.data_type, *child, ir_lid));
                } else {
                    match operand.data_type {
                        DataType::U64 => {
                            // Always display U64 as hex
                            let v = operand.val.downcast_ref::<u64>().unwrap();
                            op.push_str(&format!(" ({:?}){:#X}", operand.data_type, v));
                        }
                        DataType::Integer |
                        DataType::I64 => {
                            let v = operand.val.downcast_ref::<i64>().unwrap();
                            op.push_str(&format!(" ({:?}){}", operand.data_type, v));
                        }
                        // order matters, must be last
                        _ => {
                            let v = operand.val.downcast_ref::<String>().unwrap();
                            op.push_str(&format!(" ({:?}){}", operand.data_type, v));
                        },
                    }
                }
            }
            debug!("IRDb: {}", op);
        }
    }    
}


