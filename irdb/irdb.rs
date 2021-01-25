pub type Span = std::ops::Range<usize>;
use diags::Diags;
use lineardb::{LinOperand, LinearDb};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

use ir_base::{DataType, IR, IRKind, IROperand, OperandKind};
use std::{any::Any};

pub struct IRDb {
    pub ir_vec: Vec<IR>,
    pub parms: Vec<IROperand>,
}

impl IRDb {

    fn get_box_val(&mut self, lop: &LinOperand, diags: &mut Diags, result: &mut bool) -> Box<dyn Any> {
        match lop.data_type {
            DataType::QuotedString => {
                // Trim surround quotes and convert escape characters
                return Box::new(lop.val
                        .trim_matches('\"')
                        .to_string()
                        .replace("\\n", "\n")
                        .replace("\\t", "\t"));
            },
            DataType::Int => {
                if lop.kind == OperandKind::Constant {
                    let res = lop.val.parse::<i64>();
                    if let Ok(v) = res {
                        return Box::new(v);
                    } else {
                        *result = false;
                        let m = format!("Malformed integer operand {}", lop.val);
                        diags.err1("IR_1", &m, lop.src_loc.clone());
                        return Box::new(lop.val.clone());
                    }
                } else {
                    return Box::new(0i64);
                }
            },
            DataType::Identifier => {
                return Box::new(lop.val.clone());
            },
            DataType::Bool => {
                if lop.kind == OperandKind::Constant {
                    let res = lop.val.parse::<i64>();
                    if let Ok(v) = res {
                        if v == 0 {
                            return Box::new(false);
                        } else {
                            return Box::new(true);
                        }
                    } else {
                        *result = false;
                        let m = format!("Malformed boolean expression {}", lop.val);
                        diags.err1("IR_3", &m, lop.src_loc.clone());
                        return Box::new(lop.val.clone());
                    }
                } else {
                    return Box::new(false);
                }
            },
            DataType::Unknown => {
                let m = format!("IR conversion failed for {}", lop.val);
                diags.err1("IR_2", &m, lop.src_loc.clone());
                *result = false;
                return Box::new(lop.val.clone());
            },
        };
    }

    fn process_lin_operands(&mut self, lin_db: &LinearDb, diags: &mut Diags) -> bool {
        for lop in lin_db.operand_vec.iter() {
            let mut result = true;
            let kind = lop.kind;
            let data_type = lop.data_type;
            let src_loc = lop.src_loc.clone();
            let val = self.get_box_val(lop, diags, &mut result);
            self.parms.push(IROperand{ kind, data_type, src_loc, val});
        }

        true
    }

    // Expect 1 operand which is int or bool
    fn validate_assert_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let len = ir.operands.len();
        if len != 1 {
            let m = format!("Assert expressions must evaluate to one boolean operand, but found {} operands.", len);
            diags.err1("IR_4", &m, ir.src_loc.clone());
            return false;
        }
        let opnd = &self.parms[ir.operands[0]];
        if opnd.data_type != DataType::Int && opnd.data_type != DataType::Bool {
            let m = format!("Assert expressions requires an integer or boolean operand, found {:?}.", opnd.data_type);
            diags.err2("IR_5", &m, ir.src_loc.clone(), opnd.src_loc.clone());
            return false;
        }
        true
    }

    fn validate_operands(&self, ir: &IR, diags: &mut Diags) -> bool {
        let result = match ir.kind {
            IRKind::Assert => { self.validate_assert_operands(ir, diags) }
            IRKind::EqEq => { true }
            IRKind::Int => { true }
            IRKind::Multiply => { true }
            IRKind::Add => { true }
            IRKind::SectionStart => { true }
            IRKind::SectionEnd => { true }
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
            if self.validate_operands(&ir, diags) {
                self.ir_vec.push(ir);
            } else {
                result = false;
            }
        }
        result
    }

    pub fn new(lin_db: &LinearDb, diags: &mut Diags) -> Option<IRDb> {
        let mut ir_db = IRDb { ir_vec: Vec::new(), parms: Vec::new() };

        let mut result = ir_db.process_lin_operands(lin_db, diags);
        result &= ir_db.process_linear_ir(lin_db, diags);
        
        if !result {
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
                            let v = operand.val.downcast_ref::<i64>().unwrap();
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


