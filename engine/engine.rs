use ir_base::{IR, IRKind, DataType};
use irdb::IRDb;
use diags::Diags;
use std::{any::Any, io::Write};
use std::collections::HashMap;
use std::cell::RefCell;
use std::fs::File;
use anyhow::{Result,anyhow};

#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

#[derive(Clone,Debug,PartialEq)]
pub struct Location {
    img: usize,
    abs: usize,
    sec: usize,
}

pub struct Parameter {
    data_type: DataType,
    val: Box<dyn Any>,
}

impl Parameter {
    fn to_bool(&self) -> bool {
        match self.data_type {
            DataType::Bool => { *self.val.downcast_ref::<bool>().unwrap() },
            DataType::Int => { *self.val.downcast_ref::<i64>().unwrap() != 0 },
            _ => { assert!(false); false },
        }
    }

    fn to_i64(&self) -> i64 {
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<i64>().unwrap() },
            _ => { assert!(false); 0 },
        }
    }

    fn to_str(&self) -> &str {
        match self.data_type {
            DataType::QuotedString => { self.val.downcast_ref::<String>().unwrap() },
            _ => { assert!(false); "" },
        }
    }

}

pub struct Engine {
    parms: Vec<RefCell<Parameter>>,
    ir_locs: Vec<Location>,
    _id_locs: HashMap<String,Location>,
}

impl Engine {

    fn iterate_wrs(&mut self, ir: &IR, _diags: &mut Diags,
                    current: &mut Location) -> bool {
        trace!("Engine::iterate_wrs: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // wrs takes one input parameter
        assert!(ir.operands.len() == 1);
        let in_parm_num0 = ir.operands[0];
        let in_parm0 = self.parms[in_parm_num0].borrow();

        // If the inputs are stable, we can compute the stable output
        let sz = in_parm0.to_str().len();
        current.img += sz;
        current.abs += sz;
        current.sec += sz;
        trace!("Engine::iterate_wrs: EXIT");
        true
    }

    fn iterate_eqeq(&mut self, ir: &IR, _diags: &mut Diags,
                    current: &Location) -> bool {
        trace!("Engine::iterate_eqeq: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // eqeq takes two inputs and produces one output parameter
        assert!(ir.operands.len() == 3);
        let in_parm_num0 = ir.operands[0];
        let in_parm_num1 = ir.operands[1];
        let out_parm_num = ir.operands[2];
        let in_parm0 = self.parms[in_parm_num0].borrow();
        let in_parm1 = self.parms[in_parm_num1].borrow();
        let mut out_parm = self.parms[out_parm_num].borrow_mut();

        // If the inputs are stable, we can compute the stable output
        let in0 = in_parm0.to_i64();
        let in1 = in_parm1.to_i64();
        let out = out_parm.val.downcast_mut::<bool>().unwrap();
        *out = in0 == in1;
    
        trace!("Engine::iterate_eqeq: EXIT");
        true
    }

    fn iterate_add(&mut self, ir: &IR, _diags: &mut Diags,
                    current: &Location) -> bool {
        trace!("Engine::iterate_add: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // Takes two inputs and produces one output parameter
        assert!(ir.operands.len() == 3);
        let in_parm_num0 = ir.operands[0];
        let in_parm_num1 = ir.operands[1];
        let out_parm_num = ir.operands[2];
        let in_parm0 = self.parms[in_parm_num0].borrow();
        let in_parm1 = self.parms[in_parm_num1].borrow();
        let mut out_parm = self.parms[out_parm_num].borrow_mut();

        // If the inputs are stable, we can compute the stable output
        let in0 = in_parm0.to_i64();
        let in1 = in_parm1.to_i64();
        let out = out_parm.val.downcast_mut::<i64>().unwrap();
        *out = in0 + in1;

        trace!("Engine::iterate_add: EXIT");
        true
    }

    fn iterate_multiply(&mut self, ir: &IR, _diags: &mut Diags,
                      current: &Location) -> bool {
        trace!("Engine::iterate_multiply: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // Takes two inputs and produces one output parameter
        assert!(ir.operands.len() == 3);
        let in_parm_num0 = ir.operands[0];
        let in_parm_num1 = ir.operands[1];
        let out_parm_num = ir.operands[2];
        let in_parm0 = self.parms[in_parm_num0].borrow();
        let in_parm1 = self.parms[in_parm_num1].borrow();
        let mut out_parm = self.parms[out_parm_num].borrow_mut();

        // If the inputs are stable, we can compute the stable output
        let in0 = in_parm0.to_i64();
        let in1 = in_parm1.to_i64();
        let out = out_parm.val.downcast_mut::<i64>().unwrap();
        *out = in0 * in1;
    
        trace!("Engine::iterate_multiply: EXIT");
        true
    }    
    pub fn new(irdb: &IRDb, diags: &mut Diags, abs_start: usize) -> Engine{
        let mut engine = Engine { parms: Vec::new(), ir_locs: Vec::new(),
                                        _id_locs: HashMap::new() };
        debug!("Engine::new: ENTER");
        // Initialize parameters from the IR operands.
        engine.parms.reserve(irdb.parms.len());
        for opnd in &irdb.parms {
            let parm = Parameter { data_type: opnd.data_type,
                    val: opnd.clone_val_box() };
            engine.parms.push(RefCell::new(parm));
        }
        engine.iterate(&irdb, diags, abs_start);
        engine.dump_locations();
        debug!("Engine::new: EXIT");
        engine
    }

    fn dump_locations(&self) {
        for (idx,loc) in self.ir_locs.iter().enumerate() {
            debug!("{}: {:?}", idx, loc);
        }
    }

    pub fn iterate(&mut self, irdb: &IRDb, diags: &mut Diags, abs_start: usize) {
        trace!("Engine::iterate: abs_start = {}", abs_start);
        let mut result = true;
        let mut old_locations = Vec::new();
        let mut stable = false;
        let mut iter_count = 0;
        while result && !stable {
            trace!("Engine::iterate: Iteration count {}", iter_count);
            iter_count += 1;
            let mut current = Location{ img: 0, abs: abs_start, sec: 0 };
            for ir in &irdb.ir_vec {
                // record our location after each IR
                self.ir_locs.push(current.clone());
                result &= match ir.kind {
                    IRKind::Assert => { true /* evaluate assert at execute time */ },
                    IRKind::EqEq => { self.iterate_eqeq(&ir, diags, &current) },
                    IRKind::Int => { true /* nothing to do */ },
                    IRKind::Multiply =>{ self.iterate_multiply(&ir, diags, &current) },
                    IRKind::Add =>{ self.iterate_add(&ir, diags, &current) },
                    IRKind::Wrs => { self.iterate_wrs(&ir, diags, &mut current) },
                    IRKind::SectionStart => {
                        true // todo fix me
                    }
                    IRKind::SectionEnd => {
                        true // todo fix me
                    },
                }
            }
            if self.ir_locs == old_locations {
                stable = true;
            } else {
                // This consumes new_locations, leaving it empty
                // Is there a better way to express this?
                old_locations = self.ir_locs.drain(0..).collect();
            }
        }
    }

    fn execute_assert(&self, ir: &IR, _irdb: &IRDb, diags: &mut Diags, _file: &File)
                      -> Result<()> {
        trace!("Engine::execute_assert: ENTER");
        let mut result = Ok(());
        if self.parms[ir.operands[0]].borrow().to_bool() == false {
            let msg = format!("Assert expression failed");
            diags.err1("EXEC_2", &msg, ir.src_loc.clone());
            result = Err(anyhow!("Assert failed"));
        }
        trace!("Engine::execute_assert: EXIT");
        result
    }

    fn execute_wrs(&self, ir: &IR, _irdb: &IRDb, diags: &mut Diags, file: &mut File)
                   -> Result<()> {
        trace!("Engine::execute_wrs: ENTER");
        let buf = self.parms[ir.operands[0]].borrow();
        let bufs = buf.to_str().as_bytes();
        // the map_error lambda just converts io::error to a std::error
        let result = file.write_all(bufs)
                                     .map_err(|err|err.into());
        if result.is_err() {
            let msg = format!("Writing string failed");
            diags.err1("EXEC_3", &msg, ir.src_loc.clone());
        }
        trace!("Engine::execute_wrs: EXIT");
        result
    }

    pub fn execute(&self, irdb: &IRDb, diags: &mut Diags, file: &mut File)
                   -> Result<()> {
        trace!("Engine::execute: ENTER");
        let mut result;
        let mut error_count = 0;
        for ir in &irdb.ir_vec {
            result = match ir.kind {
                IRKind::Assert => { self.execute_assert(ir, irdb, diags, file) }
                IRKind::Wrs => { self.execute_wrs(ir, irdb, diags, file) }
                IRKind::EqEq => { Ok(()) }
                IRKind::Int => { Ok(()) }
                IRKind::Multiply => { Ok(()) }
                IRKind::Add => { Ok(()) }
                IRKind::SectionStart => { Ok(()) }
                IRKind::SectionEnd => { Ok(()) }
            };

            if result.is_err() {
                error_count += 1;
                if error_count > 10 { // todo parameterize max 10 errors
                    break;
                }
            }
        }
        trace!("Engine::execute: EXIT");
        if error_count > 0 {
            return Err(anyhow!("Error detected"));
        }
        Ok(())
    }
}