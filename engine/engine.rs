use std::{convert::TryFrom};
use ir::{DataType, IR, IRKind};
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
    img: u64,
    sec: u64,
}
pub struct Parameter {
    data_type: DataType,
    val: Box<dyn Any>,
}

impl Parameter {
    fn to_bool(&self) -> bool {
        match self.data_type {
            DataType::U64 => { *self.val.downcast_ref::<u64>().unwrap() != 0 },
            _ => panic!("Bad downcast conversion to bool!"),
        }
    }

    fn to_u64(&self) -> u64 {
        match self.data_type {
            DataType::U64 => { *self.val.downcast_ref::<u64>().unwrap() },
            _ => panic!("Bad downcast conversion to u64!"),
        }
    }

    fn to_i64(&self) -> i64 {
        match self.data_type {
            DataType::I64 => { *self.val.downcast_ref::<i64>().unwrap() },
            _ => panic!("Bad downcast conversion to i64!"),
        }
    }

    fn to_str(&self) -> &str {
        match self.data_type {
            DataType::QuotedString => { self.val.downcast_ref::<String>().unwrap() },
            _ => panic!("Bad downcast conversion to &str!"),
        }
    }

    fn to_identifier(&self) -> &str {
        match self.data_type {
            DataType::Identifier => { self.val.downcast_ref::<String>().unwrap() },
            _ => panic!("Bad downcast conversion to identifier!"),
        }
    }
}

pub struct Engine {
    parms: Vec<RefCell<Parameter>>,
    ir_locs: Vec<Location>,

    /// Stack of section offsets.  Each time processing enters
    /// a new section, we push the old section offset onto the stack
    /// and pop when return back to the parent section.
    sec_offsets: Vec<u64>,

    /// Stack of sections for debug use
    sec_names: Vec<String>,

    /// Starting absolute address, just copied from irdb for convenience
    start_addr: u64,
}

impl Engine {

    /// Debug trace that produces an indented output with section name to make
    /// section nesting more readable.
    fn trace(&self, msg: &str) {
        let mut sec_name = "";
        let sec_depth = self.sec_names.len();
        if sec_depth != 0 {
            sec_name = self.sec_names.last().unwrap();
        }
        trace!("{}{}: {}", "    ".repeat(sec_depth), sec_name, msg);
    }

    fn iterate_wrs(&mut self, ir: &IR, _irdb: &IRDb, _diags: &mut Diags,
                    current: &mut Location) -> bool {
        self.trace(format!("Engine::iterate_wrs: img {}, sec {}",
                   current.img, current.sec).as_str());
        // wrs takes one input parameter
        assert!(ir.operands.len() == 1);
        let in_parm_num0 = ir.operands[0];
        let in_parm0 = self.parms[in_parm_num0].borrow();

        // Will panic if usize does not fit in u64
        let sz = in_parm0.to_str().len() as u64;
        current.img += sz;
        current.sec += sz;
        
        true
    }

    fn do_add(&self, ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let check = in0.checked_add(in1);
        if check.is_none() {
            let msg = format!("Add expression '{} + {}' will overflow", in0, in1);
            diags.err1("EXEC_1", &msg, ir.src_loc.clone());
            false
        } else {
            *out = check.unwrap();
            true
        }
    }

    fn do_sub(&self, ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let check = in0.checked_sub(in1);
        if check.is_none() {
            let msg = format!("Subtract expression '{} - {}' will underflow", in0, in1);
            diags.err1("EXEC_4", &msg, ir.src_loc.clone());
            false
        } else {
            *out = check.unwrap();
            true
        }
    }

    fn do_mul(&self, ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let check = in0.checked_mul(in1);
        if check.is_none() {
            let msg = format!("Multiply expression '{} * {}' will overflow", in0, in1);
            diags.err1("EXEC_6", &msg, ir.src_loc.clone());
            false
        } else {
            *out = check.unwrap();
            true
        }
    }

    fn do_div(&self, ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let check = in0.checked_div(in1);
        if check.is_none() {
            let msg = format!("Exception in divide expression '{} / {}'", in0, in1);
            diags.err1("EXEC_7", &msg, ir.src_loc.clone());
            false
        } else {
            *out = check.unwrap();
            true
        }
    }

    fn do_shl(&self, ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {

        let mut result = true;
        let shift_amount = u32::try_from(in1);
        if shift_amount.is_err() {
            let msg = format!("Shift amount {} is too large in Left Shift expression '{} << {}'", in1, in0, in1);
            diags.err1("EXEC_9", &msg, ir.src_loc.clone());
            result = false;
        } else {
            *out = in0 << in1;
        }
        result
    }

    fn do_shr(&self, ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {

        let mut result = true;
        let shift_amount = u32::try_from(in1);
        if shift_amount.is_err() {
            let msg = format!("Shift amount {} is too large in Right Shift expression '{} >> {}'",
                            in1, in0, in1);
            diags.err1("EXEC_10", &msg, ir.src_loc.clone());
            result = false;
        } else {
            *out = in0 >> in1;
        }
        result
    }

    /*
    fn get_arithmetic_type(lhs: DataType, rhs: DataType, op: IRKind) -> DataType {
        let result = DataType::Unknown;
        match lhs {
            DataType::U64 => {
                match rhs {
                    DataType::U64 => return DataType::U64,
                    DataType::I64 => return DataType::U64,
                }

            }
        }
        result
    }
    */

    fn iterate_arithmetic(&mut self, ir: &IR, _irdb: &IRDb, operation: IRKind,
                    current: &Location, diags: &mut Diags) -> bool {
        self.trace(format!("Engine::iterate_arithmetic: img {}, sec {}",
                               current.img, current.sec).as_str());
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

        let result = match operation {
            IRKind::DoubleEq => { *out = (in0 == in1) as u64; true }
            IRKind::NEq => { *out = (in0 != in1) as u64; true }
            IRKind::GEq => { *out = (in0 >= in1) as u64; true }
            IRKind::LEq => { *out = (in0 <= in1) as u64; true }
            IRKind::BitAnd => { *out = in0 & in1; true }
            IRKind::LogicalAnd => { *out = ((in0 != 0) && (in1 != 0)) as u64; true }
            IRKind::BitOr => { *out = in0 | in1; true }
            IRKind::LogicalOr => { *out = ((in0 != 0) || (in1 != 0)) as u64; true }
            IRKind::Add => { self.do_add(ir, in0, in1, out, diags) }
            IRKind::Subtract => { self.do_sub(ir, in0, in1, out, diags) }
            IRKind::Multiply => { self.do_mul(ir, in0, in1, out, diags) }
            IRKind::Divide => { self.do_div(ir, in0, in1, out, diags) }
            IRKind::LeftShift => { self.do_shl(ir, in0, in1, out, diags) }
            IRKind::RightShift => { self.do_shr(ir, in0, in1, out, diags) }
            bad => {
                panic!("Called iterate_arithmetic with bad IRKind operation {:?}", bad);
            }
        };
    
        
        result
    }

    fn iterate_sizeof(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags,
                    current: &Location) -> bool {
        self.trace(format!("Engine::iterate_sizeof: img {}, sec {}",
                            current.img, current.sec).as_str());
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
        let ir_rng = irdb.sized_locs.get(sec_name);
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
        let sz = end_loc.img - start_loc.img;
        self.trace(format!("Sizeof {} is currently {}", sec_name, sz).as_str());
        // We'll at least panic at runtime if conversion from
        // usize to u64 fails instead of bad output binary.
        *out = sz.try_into().unwrap();
    
        
        true
    }

    /// Compute the transient current address.  This case is called when
    /// Abs/Img/Sec is called without an identifier.
    fn iterate_current_address(&mut self, ir: &IR, current: &Location) -> bool {
        self.trace(format!("Engine::iterate_current_address: img {}, sec {}",
                            current.img, current.sec).as_str());
        assert!(ir.operands.len() == 1);
        let out_parm_num = ir.operands[0];
        let mut out_parm = self.parms[out_parm_num].borrow_mut();
        let out = out_parm.val.downcast_mut::<u64>().unwrap();

        // We'll at least panic at runtime if conversion from
        // usize to u64 fails instead of bad output binary.
        match ir.kind {
            IRKind::Abs => { 
                // Will panic if usize does not fit in a u64
                let img: u64 = current.img.try_into().unwrap();
                *out = img + self.start_addr;
            }
            IRKind::Img => { *out = current.img.try_into().unwrap(); }
            IRKind::Sec => { *out = current.sec.try_into().unwrap(); }
            bad => {
                panic!("Called iterate_current_address with bogus IR {:?}", bad);
            }
        }
    
        
        true
    }

    /// Compute the transient address of the identifier.  This case is called when
    /// Abs/Img/Sec is called with an identifier.
    fn iterate_identifier_address(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags,
                    current: &Location) -> bool {
        self.trace(format!("Engine::iterate_identifier_address: img {}, sec {}",
                            current.img, current.sec).as_str());
        // Abs/Img/Sec take one optional input and produce one output.
        // We've already discarded surrounding () on the operand.
        assert!(ir.operands.len() == 2);
        let in_parm_num0 = ir.operands[0]; // identifier
        let out_parm_num = ir.operands[1];
        let in_parm0 = self.parms[in_parm_num0].borrow();
        let mut out_parm = self.parms[out_parm_num].borrow_mut();

        let name = in_parm0.to_identifier();
        let out = out_parm.val.downcast_mut::<u64>().unwrap();

        // We've already verified that the section identifier exists,
        // but unless the section actually got used in the output,
        // then we won't find location info for it.
        let ir_num = irdb.addressed_locs.get(name);
        if ir_num.is_none() {
            let msg = format!("Address of section or label '{}' not reachable in output.",
                    name);
            diags.err1("EXEC_11", &msg, ir.src_loc.clone());
            return false;
        }
        let ir_num = ir_num.unwrap();
        let start_loc = &self.ir_locs[*ir_num];
        match ir.kind {
            // Will panic if usize does not fit in a u64
            IRKind::Abs => {
                let img: u64 = start_loc.img.try_into().unwrap();
                *out = img + self.start_addr;
            }
            IRKind::Img => { *out = start_loc.img.try_into().unwrap(); }
            IRKind::Sec => { *out = start_loc.sec.try_into().unwrap(); }
            bad => {
                panic!("Called iterate_current_address with bogus IR {:?}", bad);
            }
        }
        
        true
    }

    fn iterate_address(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags,
                    current: &Location) -> bool {
        self.trace(format!("Engine::iterate_address: img {}, sec {}",
                            current.img, current.sec).as_str());
        // Abs/Img/SEc take one optional input and produce one output.
        // We've already discarded surrounding () on the operand.
        let num_operands = ir.operands.len();
        let result = match num_operands {
            1 => self.iterate_current_address(ir, current),
            2 => self.iterate_identifier_address(ir, irdb, diags, current),
            bad => panic!("Wrong number of IR operands = {}!", bad),
        };
        
        result
    }

    /// At the start of a section, push the old section offset
    /// and reset the current section offset to zero.
    fn iterate_section_start(&mut self, ir: &IR, irdb: &IRDb, _diags: &mut Diags,
                             current: &mut Location) -> bool {
        let sec_name = irdb.get_opnd_as_identifier(&ir, 0).to_string();
        // For debugging, push our current section on the name stack
        self.sec_names.push(sec_name);
        self.trace(format!("Engine::iterate_section_start: img {}, sec {}",
                            current.img, current.sec).as_str());
        self.sec_offsets.push(current.sec);
        current.sec = 0;
        
        true
    }

    /// At the end of a section, pop the last section offset and add
    /// its value to the current section offset
    fn iterate_section_end(&mut self, ir: &IR, irdb: &IRDb, _diags: &mut Diags,
                            current: &mut Location) -> bool {
        let sec_name = irdb.get_opnd_as_identifier(&ir, 0).to_string();
        self.trace(format!("Engine::iterate_section_end: '{}', img {}, sec {}",
                sec_name, current.img, current.sec).as_str());
        current.sec += self.sec_offsets.pop().unwrap();
        // For debugging, pop our current section from the name stack
        self.sec_names.pop();
        
        true
    }

    pub fn new(irdb: &IRDb, diags: &mut Diags, abs_start: usize) -> Option<Engine> {
        // The first iterate loop may access any IR location, so initialize all
        // ir_locs locations to zero.  
        let ir_locs = vec![Location {img: 0, sec: 0}; irdb.ir_vec.len()];

        let mut engine = Engine { parms: Vec::new(), ir_locs, sec_offsets: Vec::new(),
                                         sec_names: Vec::new(), start_addr: irdb.start_addr };
        engine.trace("Engine::new:");

        // Initialize parameters from the IR operands.
        engine.parms.reserve(irdb.parms.len());
        for opnd in &irdb.parms {
            let parm = Parameter { data_type: opnd.data_type, val: opnd.clone_val_box() };
            engine.parms.push(RefCell::new(parm));
        }


        let result = engine.iterate(&irdb, diags, abs_start);
        if !result {
            return None;
        }

        engine.trace("Engine::new: EXIT");
        Some(engine)
    }

    pub fn dump_locations(&self) {
        for (idx,loc) in self.ir_locs.iter().enumerate() {
            debug!("{}: {:?}", idx, loc);
        }
    }

    pub fn iterate(&mut self, irdb: &IRDb, diags: &mut Diags, abs_start: usize) -> bool {
        self.trace(format!("Engine::iterate: abs_start = {}", abs_start).as_str());
        let mut result = true;
        let mut old_locations = Vec::new();
        let mut stable = false;
        let mut iter_count = 0;
        while result && !stable {
            self.trace(format!("Engine::iterate: Iteration count {}", iter_count).as_str());
            iter_count += 1;
            let mut current = Location{ img: 0, sec: 0 };

            // make sure we exited as many sections as we entered on each iteration
            assert!(self.sec_offsets.len() == 0);

            for (lid,ir) in irdb.ir_vec.iter().enumerate() {
                // record our location after each IR
                self.ir_locs[lid] = current.clone();
                let operation = ir.kind;
                result &= match operation {

                    // Arithmetic with two operands in, one out
                    IRKind::Add |
                    IRKind::Subtract |
                    IRKind::RightShift |
                    IRKind::LeftShift |
                    IRKind::BitAnd |
                    IRKind::LogicalAnd |
                    IRKind::BitOr |
                    IRKind::LogicalOr |
                    IRKind::Multiply |
                    IRKind::Divide |
                    IRKind::DoubleEq |
                    IRKind::GEq |
                    IRKind::LEq |
                    IRKind::NEq => { self.iterate_arithmetic(&ir, irdb, operation, &current, diags) }

                    IRKind::Sizeof => { self.iterate_sizeof(&ir, irdb, diags, &mut current) }
                    IRKind::Wrs => { self.iterate_wrs(&ir, irdb, diags, &mut current) }
                    IRKind::Abs |
                    IRKind::Img |
                    IRKind::Sec => { self.iterate_address(ir, irdb, diags, &current) }
                    IRKind::SectionStart => { self.iterate_section_start(ir, irdb, diags, &mut current) }
                    IRKind::SectionEnd => { self.iterate_section_end(ir, irdb, diags, &mut current) }

                    // The following IR types are evaluated only at execute time.
                    // Nothing to do during iteration.
                    IRKind::Label |
                    IRKind::Assert |
                    IRKind::I64 |
                    IRKind::U64 => { true }
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

    /// If the operand is a variable, show its value.
    /// Constant operands are presumed self-evident.
    fn assert_info_operand(&self, opnd_num: usize, irdb: &IRDb, diags: &mut Diags) {
        let opnd = self.parms[opnd_num].borrow();
        let ir_opnd = &irdb.parms[opnd_num];
        match opnd.data_type {
            DataType::U64 => {
                let val = opnd.to_u64();
                let msg = format!("Operand has value {}", val);
                let primary_code_ref = ir_opnd.src_loc.clone();
                diags.note1("EXEC_8", &msg, primary_code_ref);
            }
            _ => {}
        }
    }

    /// Display addition diagnostic if the assertion occurred for an
    /// operand that is an output of another operation.
    fn assert_info(&self, src_lid: Option<usize>, irdb: &IRDb, diags: &mut Diags) {
        if src_lid.is_none() {
            // No extra info available.  Source was presumably a constant.
            return;
        }
        let src_lid = src_lid.unwrap();
        // get the operation at the source lid
        let operation = irdb.ir_vec.get(src_lid).unwrap();
        let num_operands = operation.operands.len();
        // This is an assert, so the last operation is a boolean that we
        // presume to be false, necessitating this diagnostic.
        for (idx, opnd) in operation.operands.iter().enumerate() {
            if idx < num_operands - 1 {
                self.assert_info_operand(*opnd, irdb, diags);
            }
        }
    }

    fn execute_assert(&self, ir: &IR, irdb: &IRDb, diags: &mut Diags, _file: &File)
                      -> Result<()> {
        self.trace("Engine::execute_assert:");
        let mut result = Ok(());
        let opnd_num = ir.operands[0];
        self.trace(format!("engine::execute_assert: checking operand {}", opnd_num).as_str());
        let parm = self.parms[opnd_num].borrow();
        if parm.to_bool() == false {
            // assert failed
            let msg = format!("Assert expression failed");
            diags.err1("EXEC_2", &msg, ir.src_loc.clone());

            // If the boolean the assertion failed on is an output of an operation,
            // then backtrack to print information about that operation.  To backtrack
            // we get the Option<src_lid> for the assert.
            let src_lid = irdb.get_operand_src_lid(opnd_num);
            self.assert_info(src_lid, irdb, diags);
            result = Err(anyhow!("Assert failed"));
        }
        
        result
    }

    fn execute_wrs(&self, ir: &IR, _irdb: &IRDb, diags: &mut Diags, file: &mut File)
                   -> Result<()> {
        self.trace("Engine::execute_wrs:");
        let buf = self.parms[ir.operands[0]].borrow();
        let bufs = buf.to_str().as_bytes();
        // the map_error lambda just converts io::error to a std::error
        let result = file.write_all(bufs)
                                     .map_err(|err|err.into());
        if result.is_err() {
            let msg = format!("Writing string failed");
            diags.err1("EXEC_3", &msg, ir.src_loc.clone());
        }
        
        result
    }

    pub fn execute(&self, irdb: &IRDb, diags: &mut Diags, file: &mut File)
                   -> Result<()> {
        self.trace("Engine::execute:");
        let mut result;
        let mut error_count = 0;
        for ir in &irdb.ir_vec {
            result = match ir.kind {
                IRKind::Assert => { self.execute_assert(ir, irdb, diags, file) }
                IRKind::Wrs => { self.execute_wrs(ir, irdb, diags, file) }
                // the rest of these operations are computed during iteration
                IRKind::Abs |
                IRKind::Img |
                IRKind::Sec |
                IRKind::Label |
                IRKind::Sizeof |
                IRKind::NEq |
                IRKind::GEq |
                IRKind::LEq |
                IRKind::DoubleEq |
                IRKind::I64 |
                IRKind::U64 |
                IRKind::BitAnd |
                IRKind::LogicalAnd |
                IRKind::BitOr |
                IRKind::LogicalOr |
                IRKind::Multiply |
                IRKind::Divide |
                IRKind::Add |
                IRKind::Subtract |
                IRKind::SectionStart |
                IRKind::SectionEnd |
                IRKind::LeftShift |
                IRKind::RightShift => { Ok(()) }
            };

            if result.is_err() {
                error_count += 1;
                if error_count > 10 { // todo parameterize max 10 errors
                    break;
                }
            }
        }
        
        if error_count > 0 {
            return Err(anyhow!("Error detected"));
        }
        Ok(())
    }
}