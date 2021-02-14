pub type Span = std::ops::Range<usize>;
use diags::Diags;
use lineardb::{LinOperand, LinearDb};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ir_base::{DataType, IR, IRKind, IROperand, OperandKind};
use std::{any::Any, collections::HashMap, ops::Range};
use parse_int::parse;

pub struct IRDb {
    pub ir_vec: Vec<IR>,
    pub parms: Vec<IROperand>,

    /// Maps an identifier to the (start,stop) indices in
    /// the ir_vec.
    pub id_locs: HashMap<String,Range<usize>>,
}

impl IRDb {

    fn make_box_val(&mut self, lop: &LinOperand, diags: &mut Diags) -> Option<Box<dyn Any>> {
        match lop.data_type {
            DataType::QuotedString => {
                // Trim surround quotes and convert escape characters
                return Some(Box::new(lop.val
                        .trim_matches('\"')
                        .to_string()
                        .replace("\\n", "\n")
                        .replace("\\t", "\t")));
            }
            DataType::Int => {
                if lop.kind == OperandKind::Constant {
                    let res = parse::<u64>(&lop.val);
                    if let Ok(v) = res {
                        return Some(Box::new(v));
                    } else {
                        let m = format!("Malformed integer operand {}", lop.val);
                        diags.err1("IR_1", &m, lop.src_loc.clone());
                        return None;
                    }
                } else {
                    return Some(Box::new(0u64));
                }
            }
            DataType::Identifier => {
                return Some(Box::new(lop.val.clone()));
            }
            DataType::Unknown => {
                let m = format!("IR conversion failed for {}", lop.val);
                diags.err1("IR_2", &m, lop.src_loc.clone());
                return None;
            }
        };
    }

    /// Returns the value of the specified operand for the specified IR.
    /// The operand number is for the *IR*, not the absolute operand
    /// index in the central operands vector.
    pub fn get_opnd_as_identifier(&self, ir: &IR, opnd_num: usize) -> &str {
        let &op_num = ir.operands.get(opnd_num).unwrap();
        let opnd = self.parms.get(op_num).unwrap();
        opnd.to_identifier()
    }

    fn process_lin_operands(&mut self, lin_db: &LinearDb, diags: &mut Diags) -> bool {
        for lop in lin_db.operand_vec.iter() {
            let val = self.make_box_val(lop, diags);
            if val.is_none() {
                return false;
            }
            let val = val.unwrap();
            let kind = lop.kind;
            let data_type = lop.data_type;
            let src_loc = lop.src_loc.clone();
            self.parms.push(IROperand{ kind, data_type, src_loc, val });
        }

        true
    }

    // Expect 1 operand which is int or bool
    fn validate_bool_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 {
            let m = format!("'{:?}' expressions must evaluate to one boolean operand, but found {} operands.", ir.kind, len);
            diags.err1("IR_4", &m, ir.src_loc.clone());
            return false;
        }
        let opnd = &self.parms[ir.operands[0]];
        if opnd.data_type != DataType::Int {
            let m = format!("'{:?}' expressions require an integer or boolean operand, found '{:?}'.", ir.kind, opnd.data_type);
            diags.err2("IR_5", &m, ir.src_loc.clone(), opnd.src_loc.clone());
            return false;
        }
        true
    }

    // Expect 1 operand which is int or bool
    fn validate_arithmetic_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 3 {
            let m = format!("'{:?}' expressions must evaluate to 2 input and one output operands, but found {} total operands.", ir.kind, len);
            diags.err1("IR_6", &m, ir.src_loc.clone());
            return false;
        }
        for op_num in 0..2 {
            let opnd = &self.parms[ir.operands[op_num]];
            if opnd.data_type != DataType::Int {
                let m = format!("'{:?}' expressions require an integer, found '{:?}'.", ir.kind, opnd.data_type);
                diags.err2("IR_7", &m, ir.src_loc.clone(), opnd.src_loc.clone());
                return false;
            }
        }
        true
    }

    fn validate_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let result = match ir.kind {
            IRKind::Assert => { self.validate_bool_operands(ir, diags) }
            IRKind::NEq |
            IRKind::LEq |
            IRKind::GEq |
            IRKind::DoubleEq |
            IRKind::LeftShift |
            IRKind::RightShift |
            IRKind::Multiply |
            IRKind::Divide |
            IRKind::BitAnd |
            IRKind::LogicalAnd |
            IRKind::BitOr |
            IRKind::LogicalOr |
            IRKind::Subtract |
            IRKind::Add => { self.validate_arithmetic_operands(ir, diags) }
            IRKind::U64 => { true }
            IRKind::SectionStart => { true }
            IRKind::SectionEnd => { true }
            IRKind::Sizeof => { true }
            IRKind::Wrs => { true }
        };
        result
    }

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
                    IRKind::SectionStart => {
                        // create the section entry and set the starting IR number
                        let sec_name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        let rng = Range {start: ir_num, end: 0};
                        self.id_locs.insert(sec_name, rng);
                    }
                    IRKind::SectionEnd => {
                        // Update the end of the range for this section
                        let sec_name = self.get_opnd_as_identifier(&ir, 0).to_string();
                        let rng = self.id_locs.get_mut(&sec_name).unwrap();
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
        let mut ir_db = IRDb { ir_vec: Vec::new(), parms: Vec::new(),
                                    id_locs: HashMap::new() };

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
                if operand.kind == OperandKind::Constant {
                    match operand.data_type {
                        DataType::Int => {
                            let v = operand.val.downcast_ref::<u64>().unwrap();
                            op.push_str(&format!(" ({:?} {:?}){}", operand.kind, operand.data_type, v));
                        }
                        // order matters, must be last
                        _ => {
                            let v = operand.val.downcast_ref::<String>().unwrap();
                            op.push_str(&format!(" ({:?} {:?}){}", operand.kind, operand.data_type, v));
                        },
                    }
                } else if operand.kind == OperandKind::Variable {
                    op.push_str(&format!(" ({:?} {:?})var{}", operand.kind, operand.data_type, *child));
                } else {
                    assert!(false);
                }
            }
            debug!("IRDb: {}", op);
        }
    }    
}


