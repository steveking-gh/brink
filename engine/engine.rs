use ir_base::{IR, IRKind, DataType};
use irdb::IRDb;
use diags::Diags;
use std::{any::Any, convert::TryInto, io::Write};
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
            DataType::Int => { *self.val.downcast_ref::<u64>().unwrap() != 0 },
            _ => { assert!(false); false },
        }
    }

    fn to_u64(&self) -> u64 {
        match self.data_type {
            DataType::Int => { *self.val.downcast_ref::<u64>().unwrap() },
            _ => { assert!(false); 0 },
        }
    }

    fn to_str(&self) -> &str {
        match self.data_type {
            DataType::QuotedString => { self.val.downcast_ref::<String>().unwrap() },
            _ => { assert!(false); "" },
        }
    }

    fn to_identifier(&self) -> &str {
        match self.data_type {
            DataType::Identifier => { self.val.downcast_ref::<String>().unwrap() },
            _ => { assert!(false); "" },
        }
    }
}

pub struct Engine {
    parms: Vec<RefCell<Parameter>>,
    ir_locs: Vec<Location>,
}

impl Engine {

    fn iterate_wrs(&mut self, ir: &IR, _irdb: &IRDb, _diags: &mut Diags,
                    current: &mut Location) -> bool {
        trace!("Engine::iterate_wrs: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // wrs takes one input parameter
        assert!(ir.operands.len() == 1);
        let in_parm_num0 = ir.operands[0];
        let in_parm0 = self.parms[in_parm_num0].borrow();

        let sz = in_parm0.to_str().len();
        current.img += sz;
        current.abs += sz;
        current.sec += sz;
        trace!("Engine::iterate_wrs: EXIT");
        true
    }

    fn iterate_arithmetic(&mut self, ir: &IR, _irdb: &IRDb, operation: IRKind,
                    current: &Location, diags: &mut Diags) -> bool {
        trace!("Engine::iterate_arithmetic: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // All operations here take two inputs and produces one output parameter
        assert!(ir.operands.len() == 3);
        let in_parm_num0 = ir.operands[0];
        let in_parm_num1 = ir.operands[1];
        let out_parm_num = ir.operands[2];
        let in_parm0 = self.parms[in_parm_num0].borrow();
        let in_parm1 = self.parms[in_parm_num1].borrow();
        let mut out_parm = self.parms[out_parm_num].borrow_mut();

        let in0 = in_parm0.to_u64();
        let in1 = in_parm1.to_u64();
        let out = out_parm.val.downcast_mut::<u64>().unwrap();

        let mut result = true;

        *out = match operation {
            IRKind::NEq => {
                if in0 != in1 {
                    1
                } else {
                    0
                }
            }
            IRKind::EqEq => {
                if in0 == in1 {
                    1
                } else {
                    0
                }
            }
            IRKind::Add => {
                let check = in0.checked_add(in1);
                if check.is_none() {
                    let msg = format!("Add expression '{} + {}' will overflow", in0, in1);
                    diags.err1("EXEC_1", &msg, ir.src_loc.clone());
                    result = false;
                    0
                } else {
                    check.unwrap()
                }
            }
            IRKind::Subtract => {
                let check = in0.checked_sub(in1);
                if check.is_none() {
                    let msg = format!("Subtract expression '{} - {}' will underflow", in0, in1);
                    diags.err1("EXEC_4", &msg, ir.src_loc.clone());
                    result = false;
                    0
                } else {
                    check.unwrap()
                }
            }
            IRKind::Multiply => {
                // Use checked arithmetic in case user is off the rails
                let check = in0.checked_mul(in1);
                if check.is_none() {
                    let msg = format!("Multiply expression '{} * {}' will overflow", in0, in1);
                    diags.err1("EXEC_6", &msg, ir.src_loc.clone());
                    result = false;
                    0
                } else {
                    check.unwrap()
                }
            }
            IRKind::Divide => {
                // Use checked arithmetic in case user is off the rails
                let check = in0.checked_div(in1);
                if check.is_none() {
                    let msg = format!("Bad divide expression '{} * {}'", in0, in1);
                    diags.err1("EXEC_7", &msg, ir.src_loc.clone());
                    result = false;
                    0
                } else {
                    check.unwrap()
                }
            }

            bad => {
                panic!("Called iterate_arithmetic with bad IRKind operation {:?}", bad);
            }
        };
    
        trace!("Engine::iterate_arithmetic: EXIT");
        result
    }

    fn iterate_sizeof(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags,
                    current: &Location) -> bool {
        trace!("Engine::iterate_sizeof: ENTER, abs {}, img {}, sec {}",
            current.abs, current.img, current.sec);
        // sizeof takes one input and produces one output
        // we've already discarded surrounding () on the operand
        assert!(ir.operands.len() == 2);
        let in_parm_num0 = ir.operands[0]; // identifier
        let out_parm_num = ir.operands[1];
        let in_parm0 = self.parms[in_parm_num0].borrow();
        let mut out_parm = self.parms[out_parm_num].borrow_mut();

        let sec_name = in_parm0.to_identifier();
        let out = out_parm.val.downcast_mut::<u64>().unwrap();

        // We've already verified that the section identifier exists,
        // but unless the section actually got used in the output,
        // then we won't find location info for it.
        let ir_rng = irdb.id_locs.get(sec_name);
        if ir_rng.is_none() {
            let msg = format!("Can't take sizeof() section '{}' not used in output.",
                    sec_name);
            diags.err1("EXEC_5", &msg, ir.src_loc.clone());
            return false;
        }
        let ir_rng = ir_rng.unwrap();
        assert!(ir_rng.start <= ir_rng.end);
        let start_loc = &self.ir_locs[ir_rng.start];
        let end_loc = &self.ir_locs[ir_rng.end];
        let sz = end_loc.abs - start_loc.abs;
        debug!("Sizeof {} is currently {}", sec_name, sz);
        // We'll at least panic at runtime if conversion from
        // usize to u64 fails instead of bad output binary.
        *out = sz.try_into().unwrap();
    
        trace!("Engine::iterate_sizeof: EXIT");
        true
    }

    pub fn new(irdb: &IRDb, diags: &mut Diags, abs_start: usize) -> Option<Engine> {
        // Initialize all ir_locs locations to zero.  The first iterate loop
        // may access any IR location.
        let ir_locs = vec![Location {img: 0, abs: 0, sec: 0}; irdb.ir_vec.len()];

        let mut engine = Engine { parms: Vec::new(), ir_locs };
        debug!("Engine::new: ENTER");
        // Initialize parameters from the IR operands.
        engine.parms.reserve(irdb.parms.len());
        for opnd in &irdb.parms {
            let parm = Parameter { data_type: opnd.data_type,
                    val: opnd.clone_val_box() };
            engine.parms.push(RefCell::new(parm));
        }


        let result = engine.iterate(&irdb, diags, abs_start);
        if !result {
            return None;
        }

        debug!("Engine::new: EXIT");
        Some(engine)
    }

    pub fn dump_locations(&self) {
        for (idx,loc) in self.ir_locs.iter().enumerate() {
            debug!("{}: {:?}", idx, loc);
        }
    }

    pub fn iterate(&mut self, irdb: &IRDb, diags: &mut Diags, abs_start: usize) -> bool {
        trace!("Engine::iterate: abs_start = {}", abs_start);
        let mut result = true;
        let mut old_locations = Vec::new();
        let mut stable = false;
        let mut iter_count = 0;
        while result && !stable {
            trace!("Engine::iterate: Iteration count {}", iter_count);
            iter_count += 1;
            let mut current = Location{ img: 0, abs: abs_start, sec: 0 };
            for (lid,ir) in irdb.ir_vec.iter().enumerate() {
                // record our location after each IR
                self.ir_locs[lid] = current.clone();
                let operation = ir.kind;
                result &= match operation {

                    // Arithmetic with two operands in, one out
                    IRKind::Add |
                    IRKind::Subtract |
                    IRKind::Multiply |
                    IRKind::Divide |
                    IRKind::EqEq |
                    IRKind::NEq => { self.iterate_arithmetic(&ir, irdb, operation, &current, diags) }

                    IRKind::Sizeof => { self.iterate_sizeof(&ir, irdb, diags, &mut current) }
                    IRKind::Wrs => { self.iterate_wrs(&ir, irdb, diags, &mut current) }
                    IRKind::Assert | /* evaluate assert only at execute time */
                    IRKind::U64 |
                    IRKind::SectionStart |
                    IRKind::SectionEnd => { true }
                }
            }
            if self.ir_locs == old_locations {
                stable = true;
            } else {
                // Record the current location information
                old_locations = self.ir_locs.clone();
            }
        }

        result
    }

    fn execute_assert(&self, ir: &IR, _irdb: &IRDb, diags: &mut Diags, _file: &File)
                      -> Result<()> {
        trace!("Engine::execute_assert: ENTER");
        let mut result = Ok(());
        debug!("engine::execute_assert: checking operand {}", ir.operands[0]);
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
                IRKind::Sizeof => { Ok(()) } // sizeof computed during iteration
                IRKind::NEq => { Ok(()) }
                IRKind::EqEq => { Ok(()) }
                IRKind::U64 => { Ok(()) }
                IRKind::Multiply => { Ok(()) }
                IRKind::Divide => { Ok(()) }
                IRKind::Add => { Ok(()) }
                IRKind::Subtract => { Ok(()) }
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