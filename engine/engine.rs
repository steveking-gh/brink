use IRKind::SectionStart;
use ir_base::{IR, IROperand, OperandKind, DataType, IRKind};
use irdb::IRDb;
use diags::Diags;
use std::any::Any;
use std::collections::HashMap;
use std::convert::From;


#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

pub struct Location {
    img: usize,
    abs: usize,
    sec: usize,
    size: usize,
}

pub struct Parameter {
    stable: bool,
    data_type: DataType,
    val: Box<dyn Any>,
}

impl Parameter {
    fn to_bool(&self) -> bool {
        // Value is undetermined before stablity
        assert!(self.stable);
        match self.data_type {
            DataType::Bool => { *self.val.downcast_ref::<bool>().unwrap() },
            DataType::Int => { *self.val.downcast_ref::<i64>().unwrap() != 0 },
            _ => { assert!(false); false },
        }
    }

    fn to_i64(&self) -> i64 {
        // Value is undetermined before stablity
        assert!(self.stable);
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<i64>().unwrap() },
            _ => { assert!(false); 0 },
        }
    }
}

pub struct Engine {
    parms: Vec<Parameter>,
    ir_locs: Vec<Location>,
    id_locs: HashMap<String,Location>,
}

impl Engine {

    /// Process an assert statement if the boolean operand is stable.
    fn process_assert(&mut self, ir: &IR, diags: &mut Diags,
                      current: &Location) -> bool {
        trace!("Engine::process_assert: ENTER");
        // assert takes a single boolean parameter
        assert!(ir.operands.len() == 1);
        let parm_num = ir.operands[0];
        let parm = &self.parms[parm_num];
        if parm.stable && parm.to_bool() == false {
            let m = format!("assert failed");
            diags.err1("EXEC_1", &m, ir.src_loc.clone());
            return false;
        }
    
        trace!("Engine::process_assert: EXIT");
        true
    }

    fn process_eqeq(&mut self, ir: &IR, diags: &mut Diags,
                      current: &Location) -> bool {
        trace!("Engine::process_eqeq: ENTER");
        // assert takes a single boolean parameter
        assert!(ir.operands.len() == 3);
        let in_parm_num0 = ir.operands[0];
        let in_parm_num1 = ir.operands[1];
        let out_parm_num = ir.operands[2];
        let in_parm0 = &self.parms[in_parm_num0];
        let in_parm1 = &self.parms[in_parm_num1];
        let mut out_parm = &mut self.parms[out_parm_num];

        // If the inputs are stable, we can compute the stable output
        if in_parm0.stable && in_parm1.stable {
            let in0 = in_parm0.to_i64();
            let in1 = in_parm1.to_i64();
            let out = out_parm.val.downcast_ref::<bool>().unwrap();
            *out = in0 == in1;
            out_parm.stable = true;
        }
    
        trace!("Engine::process_eqeq: EXIT");
        true
    }

    pub fn new(irdb: &IRDb, diags: &mut Diags, abs_start: usize) {
        let mut engine = Engine { parms: Vec::new(), ir_locs: Vec::new(),
                                  id_locs: HashMap::new() };
        debug!("Engine::new: ENTER");
        // Initialize parameters from the IR operands.
        engine.parms.reserve(irdb.parms.len());
        for opnd in &irdb.parms {
            let stable = if opnd.kind == OperandKind::Constant
                        { true } else { false };
            let parm = Parameter { stable, data_type: opnd.data_type, val: opnd.val };
            engine.parms.push(parm);
            
        }
        engine.iterate(&irdb, diags, abs_start);
        debug!("Engine::new: EXIT");
    }

    pub fn iterate(&mut self, irdb: &IRDb, diags: &mut Diags, abs_start: usize) {
        debug!("Engine::iterate: abs_start = {}", abs_start);
        let mut current = Location{ img: 0, abs: abs_start, sec: 0, size: 0 };
        let mut result = true;
        for ir in &irdb.ir_vec {
            result &= match ir.kind {
                Assert => { self.process_assert(&ir, diags, &current) },
                EqEq => { self.process_eqeq(&ir, diags, &current) },
                Int => {
                    true // just an integer, nothing to do
                },
                Multiply => {
                    true // todo fix me
                },
                Add => {
                    true // todo fix me
                },
                Wrs => {
                    true // todo fix me
                },                
                SectionStart => {
                    true // todo fix me
                },
                SectionEnd => {
                    true // todo fix me
                },
            }
        }
    }    
}