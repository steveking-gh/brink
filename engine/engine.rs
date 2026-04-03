// Iterative evaluator and binary executor for brink.
//
// Engine is the fourth and final stage of the compiler pipeline.  Because
// sections can reference the sizes and addresses of other sections that appear
// later in the output, a single linear pass is not enough to resolve all
// values.  Engine therefore runs an iterate loop, re-evaluating every IR
// instruction until all location-counter values stabilize.  Once stable,
// Engine runs an execute pass that writes the actual binary output — padding
// bytes, inline data, and the contents of referenced files — to the output
// file.  Errors detected during either pass are reported through Diags.
//
// Order of operations: engine runs last, after irdb has produced a fully
// typed and validated IRDb.  Its output is the finished binary output file.

use anyhow::{Result, anyhow};
use diags::{Diags, SourceSpan};
use ext::{ExtensionRegistry, RegisteredExtension};
use ir::{ConstBuiltins, DataType, IR, IRKind, ParameterValue};
use irdb::IRDb;
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::{convert::TryFrom, io::Read};

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// Tracks address ranges written during the execute phase.
/// Maps `start_addr -> (end_addr_exclusive, src_loc)`.
type WrittenRanges = BTreeMap<u64, (u64, SourceSpan)>;

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    /// Total bytes written to the output file.  Internal use only; drives
    /// WrDispatch offsets and mmap slicing.  Never resets.
    file_offset: u64,
    /// Offset from the most recent `set_addr` base (or `start_addr` if
    /// `set_addr` has never been called).  Exposed to scripts as `addr_offset()`.
    /// Resets to 0 on each `set_addr` call.
    addr_offset: u64,
    /// Offset within the current section.  Pushed/popped at section boundaries.
    sec_offset: u64,
    /// The address base established by the last `set_addr` call, or
    /// `start_addr` at image start.  `addr() == addr_base + addr_offset`.
    addr_base: u64,
}

/// Records one occurrence of a section write in the output image.
/// A section written N times via `wr` produces N `WrDispatch` entries,
/// each at a distinct `file_offset`, in output order.
#[derive(Clone, Debug)]
pub struct WrDispatch {
    pub name: String,
    /// Byte offset from the start of the output file where this section begins.
    pub file_offset: u64,
    /// Offset from the most recent `set_addr` anchor at the point this section begins.
    pub addr_offset: u64,
    /// Address at the point this section begins (`addr_base + addr_offset`).
    pub addr: u64,
    pub size: u64,
}

/// Records the output-image position of a label.
#[derive(Clone, Debug)]
pub struct LabelDispatch {
    pub name: String,
    /// Byte offset from the start of the output file where this label appears.
    pub file_offset: u64,
    /// Offset from the most recent `set_addr` anchor at this label.
    pub addr_offset: u64,
    /// Absolute address at this label (`abs_base + addr_offset`).
    pub addr: u64,
}

/// All parent-scope state saved on section entry and restored on section exit.
struct ScopeFrame {
    /// Parent's section-relative byte offset, restored on exit.
    sec_offset: u64,
    /// Parent's address base (set by `set_addr` or inherited from start).
    /// Restored on exit so a child's `set_addr` does not leak into the parent.
    addr_base: u64,
    /// Parent's address offset, advanced by the child's byte count on exit.
    addr_offset: u64,
    /// Section name, used for trace indentation.
    sec_name: String,
    /// True if `set_addr` fired mid-scope (sec_offset != 0 at the call site).
    /// Arms the EXEC_54 warning for subsequent set_sec_offset / set_addr_offset.
    set_addr_seen: bool,
}

pub struct Engine {
    parms: Vec<ParameterValue>,
    ir_locs: Vec<Location>,

    /// One frame per active section, innermost last.  Pushed on SectionStart,
    /// popped on SectionEnd.  Replaces the formerly separate sec_offsets,
    /// sec_names, and set_addr_in_scope vecs.
    scope_stack: Vec<ScopeFrame>,

    /// (lid, code) pairs for which a warning has already been emitted.
    /// Keyed by both index and code so distinct warnings on the same IR
    /// instruction are deduplicated independently.  Prevents duplicate
    /// diagnostics across iterate passes.
    warned_lids: HashSet<(usize, &'static str)>,

    /// Starting absolute address, just copied from irdb for convenience.
    pub start_addr: u64,

    /// One entry per section write in output order, including repeated writes.
    /// Populated by `build_dispatches` after iterate converges.
    pub wr_dispatches: Vec<WrDispatch>,

    /// One entry per label in output order.
    /// Populated by `build_dispatches` after iterate converges.
    pub label_dispatches: Vec<LabelDispatch>,
}

fn get_wrx_byte_width(ir: &IR) -> usize {
    match ir.kind {
        IRKind::Wr(w) => w as usize,
        bad => {
            panic!("Called get_wrx_byte_width with {:?}", bad);
        }
    }
}

impl Engine {
    /// Debug trace that produces an indented output with section name to make
    /// section nesting more readable.
    fn trace(&self, msg: &str) {
        let sec_depth = self.scope_stack.len();
        let sec_name = self
            .scope_stack
            .last()
            .map(|f| f.sec_name.as_str())
            .unwrap_or("");
        trace!("{}{}: {}", "    ".repeat(sec_depth), sec_name, msg);
    }

    fn iterate_wrs(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &mut Location,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_wrs: file_pos {}, off {}, sec {}",
                current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );

        let xstr_opt = self.evaluate_string_expr(ir, irdb, diags);
        if xstr_opt.is_none() {
            return false;
        }

        let xstr = xstr_opt.unwrap();

        // Will panic if usize does not fit in u64
        let sz = xstr.len() as u64;

        let Some(new_file_pos) = current.file_offset.checked_add(sz) else {
            diags.err1(
                "EXEC_41",
                "Write operation causes location counter overflow",
                ir.src_loc.clone(),
            );
            return false;
        };
        let new_off = current.addr_offset + sz; // safe: off <= file_pos, so if file_pos+sz didn't overflow, this won't
        if current.addr_base.checked_add(new_off).is_none() {
            diags.err1(
                "EXEC_43",
                "Write operation causes absolute address overflow",
                ir.src_loc.clone(),
            );
            return false;
        }

        current.file_offset = new_file_pos;
        current.addr_offset = new_off;
        current.sec_offset = current.sec_offset.saturating_add(sz);

        true
    }

    /// Evaluates a dynamic `wr` statement specifically targeting compiled `BrinkExtension` traits.
    /// We check the extension registry to find the fixed length payload size.
    fn iterate_wrext(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        current: &mut Location,
        ext_registry: &ExtensionRegistry,
        diags: &mut Diags,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_wrext: file_pos {}, off {}, sec {}",
                current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );

        let opnd_idx = ir.operands[0];
        let opnd = &irdb.parms[opnd_idx];

        // The operand to a `wr` statement isn't a direct identifier; it evaluates downwards
        // to essentially an unresolved IR node mapping point. We use `is_output_of()` to crawl backwards
        // up the instruction's dependency graph to find the `ExtensionCall` IR node that produced this
        // target mapping. From there, we extract the extension's string name.
        let mut ext_name_for_diag = "<unknown>";
        if let Some(prod_ir_idx) = opnd.is_output_of() {
            let prod_ir = &irdb.ir_vec[prod_ir_idx];
            if matches!(
                prod_ir.kind,
                IRKind::ExtensionCall
                    | IRKind::ExtensionCallRanged
                    | IRKind::ExtensionCallSection
            ) {
                let ext_name_opnd = &irdb.parms[prod_ir.operands[0]];
                let ext_name = ext_name_opnd.val.to_identifier();
                ext_name_for_diag = ext_name;
                if let Some(entry) = ext_registry.get(ext_name) {
                    let size = entry.cached_size as u64;
                    current.file_offset += size;
                    current.addr_offset += size;
                    current.sec_offset += size;
                    return true;
                }
            }
        }

        diags.err1(
            "EXEC_50",
            &format!(
                "Failed to resolve extension '{}' size during layout.",
                ext_name_for_diag
            ),
            ir.src_loc.clone(),
        );
        false
    }

    // Used for Wr8 though Wr64
    fn iterate_wrx(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &mut Location,
    ) -> bool {
        assert!(ir.operands.len() < 3);
        let byte_size = get_wrx_byte_width(ir) as u64;

        self.trace(
            format!(
                "Engine::iterate_wrx-{}: file_pos {}, off {}, sec {}",
                byte_size * 8,
                current.file_offset,
                current.addr_offset,
                current.sec_offset
            )
            .as_str(),
        );

        let mut result = true;

        // determine the optional repeat count value
        let mut repeat_count = 1;
        if ir.operands.len() == 2 {
            // Yes, we have a repeat count
            // A repeat count of 0 is not an error.
            let op = &self.parms[ir.operands[1]];
            match op.data_type() {
                DataType::U64 => {
                    repeat_count = op.to_u64();
                }
                DataType::Integer | DataType::I64 => {
                    let temp = op.to_i64();
                    if temp < 0 {
                        let msg = format!(
                            "Repeat count cannot be negative, \
                                                but found '{}'",
                            temp
                        );
                        let src_loc = irdb.parms[ir.operands[1]].src_loc.clone();
                        diags.err1("EXEC_32", &msg, src_loc);
                        result = false;
                        repeat_count = 0;
                    } else {
                        repeat_count = op.to_u64();
                    }
                }
                bad => {
                    let msg = format!("Repeat count cannot be type '{:?}'", bad);
                    let src_loc = irdb.parms[ir.operands[1]].src_loc.clone();
                    diags.err1("EXEC_31", &msg, src_loc);
                    result = false;
                }
            }
        }

        // total size is the size of the wrx times the optional repeat count
        let Some(sz) = byte_size.checked_mul(repeat_count) else {
            let src_loc = irdb.parms[if ir.operands.len() == 2 {
                ir.operands[1]
            } else {
                ir.operands[0]
            }]
            .src_loc
            .clone();
            diags.err1(
                "EXEC_36",
                "Write repeat count causes size overflow",
                src_loc,
            );
            return false;
        };

        self.trace(format!("Engine::iterate_wrx-{}: size is {}", byte_size * 8, sz).as_str());

        // Guard against overflow on the file position counter
        let Some(new_file_pos) = current.file_offset.checked_add(sz) else {
            diags.err1(
                "EXEC_37",
                "Write operation causes location counter overflow",
                ir.src_loc.clone(),
            );
            return false;
        };
        let new_off = current.addr_offset + sz; // safe: off <= file_pos
        if current.addr_base.checked_add(new_off).is_none() {
            diags.err1(
                "EXEC_43",
                "Write operation causes absolute address overflow",
                ir.src_loc.clone(),
            );
            return false;
        }

        current.file_offset = new_file_pos;
        current.addr_offset = new_off;
        current.sec_offset = current.sec_offset.saturating_add(sz);

        result
    }

    /// Used for wr file
    /// There is nothing really to iterate other than advancing
    /// the location counter by the size of the file.
    fn iterate_wrf(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &mut Location,
    ) -> bool {
        // The operand is a file path
        assert!(ir.operands.len() < 2);

        let path_opnd = &self.parms[ir.operands[0]];
        let file_path = path_opnd.to_str();

        // we already verified this is a legit file path,
        // so unwrap is ok.
        let file_info = irdb.files.get(file_path).unwrap();

        let byte_size = file_info.size;

        self.trace(
            format!(
                "Engine::iterate_wrf '{}' with size {}: \
                                file_pos {}, off {}, sec {}",
                file_path, byte_size, current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );

        let Some(new_file_pos) = current.file_offset.checked_add(byte_size) else {
            diags.err1(
                "EXEC_40",
                "Write operation causes location counter overflow",
                ir.src_loc.clone(),
            );
            return false;
        };
        let new_off = current.addr_offset + byte_size; // safe: off <= file_pos
        if current.addr_base.checked_add(new_off).is_none() {
            diags.err1(
                "EXEC_43",
                "Write operation causes absolute address overflow",
                ir.src_loc.clone(),
            );
            return false;
        }

        current.file_offset = new_file_pos;
        current.addr_offset = new_off;
        current.sec_offset = current.sec_offset.saturating_add(byte_size);

        true
    }

    /// Compute the string representation of the expression.
    /// Returns the resulting string in xstr.
    /// If the diags noprint option is true, suppress printing.
    /// Returns None of failure
    fn evaluate_string_expr(&self, ir: &IR, irdb: &IRDb, diags: &mut Diags) -> Option<String> {
        let mut result = true;
        let mut xstr = String::new();
        for (local_op_num, &op_num) in ir.operands.iter().enumerate() {
            let op = &self.parms[op_num];
            debug!(
                "Processing string expr operand {} with data type {:?}",
                local_op_num,
                op.data_type()
            );
            match op.data_type() {
                DataType::QuotedString => {
                    xstr.push_str(op.to_str());
                }
                DataType::U64 => {
                    xstr.push_str(format!("{:#X}", op.to_u64()).as_str());
                }
                DataType::Integer | DataType::I64 => {
                    xstr.push_str(format!("{}", op.to_i64()).as_str());
                }
                bad => {
                    let msg = format!("Cannot stringify type '{:?}'", bad);
                    let src_loc = irdb.parms[op_num].src_loc.clone();
                    diags.err1("EXEC_14", &msg, src_loc);
                    result = false;
                }
            }
        }

        // If stringifying succeeded, return the String
        if result { Some(xstr) } else { None }
    }

    fn do_u64_add(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_add(in1) else {
            let msg = format!("Add expression '{in0} + {in1}' will overflow type U64");
            diags.err1("EXEC_1", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_i64_add(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_add(in1) else {
            let msg = format!("Add expression '{in0} + {in1}' will overflow type I64");
            diags.err1("EXEC_21", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_u64_sub(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_sub(in1) else {
            let msg = format!("Subtract expression '{in0} - {in1}' will underflow type U64");
            diags.err1("EXEC_4", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_i64_sub(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_sub(in1) else {
            let msg = format!("Subtract expression '{in0} - {in1}' will underflow type I64");
            diags.err1("EXEC_24", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_u64_mul(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_mul(in1) else {
            let msg = format!("Multiply expression '{in0} * {in1}' will overflow type U64");
            diags.err1("EXEC_6", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_i64_mul(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_mul(in1) else {
            let msg = format!("Multiply expression '{in0} * {in1}' will overflow data type I64");
            diags.err1("EXEC_26", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_u64_div(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_div(in1) else {
            let msg = format!("Exception in divide expression '{in0} / {in1}'");
            diags.err1("EXEC_7", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_u64_mod(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_rem(in1) else {
            let msg = format!("Exception in modulo expression '{in0} % {in1}'");
            diags.err1("EXEC_28", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_i64_div(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_div(in1) else {
            let msg = format!("Exception in divide expression '{in0} / {in1}'");
            diags.err1("EXEC_27", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_i64_mod(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Some(checked_result) = in0.checked_rem(in1) else {
            let msg = format!("Exception in modulo expression '{in0} % {in1}'");
            diags.err1("EXEC_30", &msg, ir.src_loc.clone());
            return false;
        };
        *out = checked_result;
        true
    }

    fn do_u64_shl(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Ok(shift_amount) = u32::try_from(in1) else {
            let msg = format!(
                "Shift amount {in1} is too large in Left Shift expression '{in0} << {in1}'"
            );
            diags.err1("EXEC_9", &msg, ir.src_loc.clone());
            return false;
        };
        *out = in0.checked_shl(shift_amount).unwrap_or(0);
        true
    }

    fn do_i64_shl(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Ok(shift_amount) = u32::try_from(in1) else {
            let msg = format!(
                "Shift amount {in1} is too large in Left Shift expression '{in0} << {in1}'"
            );
            diags.err1("EXEC_29", &msg, ir.src_loc.clone());
            return false;
        };
        *out = in0.checked_shl(shift_amount).unwrap_or(0);
        true
    }

    fn do_u64_shr(ir: &IR, in0: u64, in1: u64, out: &mut u64, diags: &mut Diags) -> bool {
        let Ok(shift_amount) = u32::try_from(in1) else {
            let msg = format!(
                "Shift amount {in1} is too large in Right Shift expression '{in0} >> {in1}'"
            );
            diags.err1("EXEC_10", &msg, ir.src_loc.clone());
            return false;
        };
        *out = in0.checked_shr(shift_amount).unwrap_or(0);
        true
    }

    fn do_i64_shr(ir: &IR, in0: i64, in1: i64, out: &mut i64, diags: &mut Diags) -> bool {
        let Ok(shift_amount) = u32::try_from(in1) else {
            let msg = format!(
                "Shift amount {in1} is too large in Right Shift expression '{in0} >> {in1}'"
            );
            diags.err1("EXEC_20", &msg, ir.src_loc.clone());
            return false;
        };
        *out = in0.checked_shr(shift_amount).unwrap_or(0);
        true
    }

    fn iterate_type_conversion(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        operation: IRKind,
        current: &Location,
        diags: &mut Diags,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_type_conversion: file_pos {}, sec {}",
                current.file_offset, current.sec_offset
            )
            .as_str(),
        );
        // All operations here take one input and produce one output parameter
        let mut result = true;
        assert!(ir.operands.len() == 2);
        let in_parm_num0 = ir.operands[0];
        let out_parm_num = ir.operands[1];
        let in_parm0 = self.parms[in_parm_num0].clone();
        let out_parm = &mut self.parms[out_parm_num];
        match operation {
            IRKind::ToU64 => {
                let out = out_parm.to_u64_mut();
                match in_parm0.data_type() {
                    DataType::U64 => {
                        // Trivial Integer or U64 to U64
                        let in0 = in_parm0.to_u64();
                        *out = in0;
                    }
                    DataType::Integer | DataType::I64 => {
                        // I64 to U64
                        let in0 = in_parm0.to_i64();
                        *out = in0 as u64;
                    }
                    bad => {
                        let src_loc = irdb.parms[in_parm_num0].src_loc.clone();
                        let msg = format!("Can't convert from {bad:?} to U64");
                        diags.err1("EXEC_17", &msg, src_loc);
                        result = false;
                    }
                }
            }
            IRKind::ToI64 => {
                let out = out_parm.to_i64_mut();
                match in_parm0.data_type() {
                    DataType::U64 => {
                        // U64 to I64
                        let in0 = in_parm0.to_u64();
                        *out = in0 as i64;
                    }
                    DataType::Integer | DataType::I64 => {
                        // Trivial Integer or I64 to I64
                        let in0 = in_parm0.to_i64();
                        *out = in0;
                    }
                    bad => {
                        let src_loc = irdb.parms[in_parm_num0].src_loc.clone();
                        let msg = format!("Can't convert from {bad:?} to I64");
                        diags.err1("EXEC_12", &msg, src_loc);
                        result = false;
                    }
                }
            }

            bad => {
                panic!("Called iterate_type_conversion with bad IRKind operation {bad:?}");
            }
        }
        result
    }

    fn iterate_arithmetic(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        operation: IRKind,
        current: &Location,
        diags: &mut Diags,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_arithmetic: file_pos {}, sec {}",
                current.file_offset, current.sec_offset
            )
            .as_str(),
        );
        // All operations here take two inputs and produces one output parameter
        assert!(ir.operands.len() == 3);

        // Borrow the parameters from the main array
        let lhs_num = ir.operands[0];
        let rhs_num = ir.operands[1];
        let out_num = ir.operands[2];
        let lhs = &self.parms[lhs_num];
        let rhs = &self.parms[rhs_num];

        let lhs_dt = lhs.data_type();
        let rhs_dt = rhs.data_type();

        if lhs_dt != rhs_dt {
            let mut dt_ok = false;
            // Right and left side data types are not equal.
            // Determine if we can proceed.
            if rhs_dt == DataType::Integer {
                if [DataType::I64, DataType::U64, DataType::Integer].contains(&lhs_dt) {
                    dt_ok = true; // Integers work with s/u types
                }
            } else if lhs_dt == DataType::Integer
                && [DataType::I64, DataType::U64].contains(&rhs_dt)
            {
                dt_ok = true; // Integers work with s/u types
            }

            if !dt_ok {
                let loc0 = irdb.parms[lhs_num].src_loc.clone();
                let loc1 = irdb.parms[rhs_num].src_loc.clone();
                let msg = format!(
                    "Input operand types do not match.  Left is '{:?}', right is '{:?}'",
                    lhs_dt, rhs_dt
                );
                diags.err2("EXEC_13", &msg, loc0, loc1);
                return false;
            }
        }

        let mut result = true;
        // output of compare is u64 regardless of inputs
        // check both parms since one might be an ambiguous integer
        // If either side is unsigned, the whole thing is unsigned
        if (lhs_dt == DataType::U64) || (rhs_dt == DataType::U64) {
            let in0 = lhs.to_u64();
            let in1 = rhs.to_u64();
            let out_parm = &mut self.parms[out_num];
            let out = out_parm.to_u64_mut();

            match operation {
                IRKind::DoubleEq => *out = (in0 == in1) as u64,
                IRKind::NEq => *out = (in0 != in1) as u64,
                IRKind::GEq => *out = (in0 >= in1) as u64,
                IRKind::LEq => *out = (in0 <= in1) as u64,
                IRKind::Gt => *out = (in0 > in1) as u64,
                IRKind::Lt => *out = (in0 < in1) as u64,
                IRKind::BitAnd => *out = in0 & in1,
                IRKind::LogicalAnd => *out = ((in0 != 0) && (in1 != 0)) as u64,
                IRKind::BitOr => *out = in0 | in1,
                IRKind::LogicalOr => *out = ((in0 != 0) || (in1 != 0)) as u64,
                IRKind::Add => {
                    result &= Engine::do_u64_add(ir, in0, in1, out, diags);
                }
                IRKind::Subtract => {
                    result &= Engine::do_u64_sub(ir, in0, in1, out, diags);
                }
                IRKind::Multiply => {
                    result &= Engine::do_u64_mul(ir, in0, in1, out, diags);
                }
                IRKind::Divide => {
                    result &= Engine::do_u64_div(ir, in0, in1, out, diags);
                }
                IRKind::Modulo => {
                    result &= Engine::do_u64_mod(ir, in0, in1, out, diags);
                }
                IRKind::LeftShift => {
                    result &= Engine::do_u64_shl(ir, in0, in1, out, diags);
                }
                IRKind::RightShift => {
                    result &= Engine::do_u64_shr(ir, in0, in1, out, diags);
                }
                bad => panic!("Forgot to handle u64 {:?}", bad),
            };
        } else if (lhs_dt == DataType::I64)
            || (rhs_dt == DataType::I64)
            || ((lhs_dt == DataType::Integer) && (rhs_dt == DataType::Integer))
        {
            // If either side is signed, treat the whole expression as signed
            // If both sides are ambiguous integers then treat the whole expression as signed
            let in0 = lhs.to_i64();
            let in1 = rhs.to_i64();
            let out_parm = &mut self.parms[out_num];

            match operation {
                // output of compare is u64 regardless of inputs
                IRKind::LogicalAnd => {
                    let out = out_parm.to_u64_mut();
                    *out = ((in0 != 0) && (in1 != 0)) as u64
                }
                IRKind::LogicalOr => {
                    let out = out_parm.to_u64_mut();
                    *out = ((in0 != 0) || (in1 != 0)) as u64
                }
                IRKind::LEq => {
                    let out = out_parm.to_u64_mut();
                    *out = (in0 <= in1) as u64
                }
                IRKind::GEq => {
                    let out = out_parm.to_u64_mut();
                    *out = (in0 >= in1) as u64
                }
                IRKind::Gt => {
                    let out = out_parm.to_u64_mut();
                    *out = (in0 > in1) as u64
                }
                IRKind::Lt => {
                    let out = out_parm.to_u64_mut();
                    *out = (in0 < in1) as u64
                }
                IRKind::NEq => {
                    let out = out_parm.to_u64_mut();
                    *out = (in0 != in1) as u64
                }
                IRKind::DoubleEq => {
                    let out = out_parm.to_u64_mut();
                    *out = (in0 == in1) as u64
                }

                IRKind::BitOr => {
                    let out = out_parm.to_i64_mut();
                    *out = in0 | in1
                }
                IRKind::BitAnd => {
                    let out = out_parm.to_i64_mut();
                    *out = in0 & in1
                }
                IRKind::Add => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_add(ir, in0, in1, out, diags);
                }
                IRKind::Subtract => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_sub(ir, in0, in1, out, diags);
                }
                IRKind::Multiply => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_mul(ir, in0, in1, out, diags);
                }
                IRKind::Divide => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_div(ir, in0, in1, out, diags);
                }
                IRKind::Modulo => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_mod(ir, in0, in1, out, diags);
                }
                IRKind::LeftShift => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_shl(ir, in0, in1, out, diags);
                }
                IRKind::RightShift => {
                    let out = out_parm.to_i64_mut();
                    result &= Engine::do_i64_shr(ir, in0, in1, out, diags);
                }

                bad => panic!("Forgot to handle i64 {:?}", bad),
            }
        } else {
            let loc0 = irdb.parms[lhs_num].src_loc.clone();
            let loc1 = irdb.parms[rhs_num].src_loc.clone();
            // check above ensures the types are the same, whatever they are
            let msg = format!(
                "Unexpected input operand types '{:?}'  Expected I64 or U64.",
                lhs_dt
            );
            diags.err2("EXEC_19", &msg, loc0, loc1);
            return false;
        }
        result
    }

    fn iterate_sizeof(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &Location,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_sizeof: file_pos {}, sec {}",
                current.file_offset, current.sec_offset
            )
            .as_str(),
        );
        // sizeof takes one input and produces one output
        // we've already discarded surrounding () on the operand
        assert!(ir.operands.len() == 2);
        let in_parm_num0 = ir.operands[0]; // identifier
        let out_parm_num = ir.operands[1];

        let in_parm = &self.parms[in_parm_num0];

        if in_parm.data_type() == DataType::Identifier {
            let sec_name = in_parm.to_identifier().to_string();

            // We've already verified that the section identifier exists,
            // but unless the section actually got used in the output,
            // then we won't find location info for it.
            let ir_rng = irdb.sized_locs.get(&sec_name);
            if ir_rng.is_none() {
                let msg = format!(
                    "Can't take sizeof() section '{}' not used in output.",
                    sec_name
                );
                diags.err1("EXEC_5", &msg, ir.src_loc.clone());
                return false;
            }
            let ir_rng = ir_rng.unwrap();
            assert!(ir_rng.start <= ir_rng.end);
            let start_loc = &self.ir_locs[ir_rng.start];
            let end_loc = &self.ir_locs[ir_rng.end];

            if start_loc.file_offset > end_loc.file_offset {
                self.trace(
                    format!(
                        "Starting file_pos {} > ending file_pos {} in {}",
                        start_loc.file_offset, end_loc.file_offset, sec_name
                    )
                    .as_str(),
                );
                *self.parms[out_parm_num].to_u64_mut() = 0;
            } else {
                let sz: u64 = end_loc.file_offset - start_loc.file_offset;
                self.trace(format!("Sizeof {} is currently {}", sec_name, sz).as_str());
                *self.parms[out_parm_num].to_u64_mut() = sz;
            }
            return true;
        }

        diags.err1(
            "EXEC_52",
            "sizeof() only accepts section names.",
            ir.src_loc.clone(),
        );
        false
    }

    fn iterate_sizeof_ext(
        &mut self,
        ir: &IR,
        diags: &mut Diags,
        ext_registry: &ExtensionRegistry,
    ) -> bool {
        // SizeofExt has 2 operands: [0] = extension name (Identifier), [1] = output U64
        assert!(ir.operands.len() == 2);
        let out_parm_num = ir.operands[1];
        let name = self.parms[ir.operands[0]].to_identifier().to_string();
        if let Some(entry) = ext_registry.get(&name) {
            *self.parms[out_parm_num].to_u64_mut() = entry.cached_size as u64;
            true
        } else {
            diags.err1(
                "EXEC_53",
                &format!("Unknown extension '{}' in sizeof().", name),
                ir.src_loc.clone(),
            );
            false
        }
    }

    fn iterate_output_size(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags) -> bool {
        // __OUTPUT_SIZE has no input operands, only a single output operand.
        assert!(ir.operands.len() == 1);
        let out_parm_num = ir.operands[0];
        let sec_name = &irdb.output_sec_str;

        let ir_rng = irdb.sized_locs.get(sec_name);
        if ir_rng.is_none() {
            let msg = format!("__OUTPUT_SIZE: output section '{}' not found.", sec_name);
            diags.err1("EXEC_57", &msg, ir.src_loc.clone());
            return false;
        }
        let ir_rng = ir_rng.unwrap();
        assert!(ir_rng.start <= ir_rng.end);
        let start_loc = &self.ir_locs[ir_rng.start];
        let end_loc = &self.ir_locs[ir_rng.end];

        if start_loc.file_offset > end_loc.file_offset {
            *self.parms[out_parm_num].to_u64_mut() = 0;
        } else {
            *self.parms[out_parm_num].to_u64_mut() = end_loc.file_offset - start_loc.file_offset;
        }
        true
    }

    fn iterate_output_addr(&mut self, ir: &IR, irdb: &IRDb, diags: &mut Diags) -> bool {
        // __OUTPUT_ADDR has no input operands, only a single output operand.
        assert!(ir.operands.len() == 1);
        let out_parm_num = ir.operands[0];
        let sec_name = &irdb.output_sec_str;

        let ir_num = irdb.addressed_locs.get(sec_name);
        if ir_num.is_none() {
            let msg = format!(
                "__OUTPUT_ADDR: output section '{}' not reachable.",
                sec_name
            );
            diags.err1("EXEC_58", &msg, ir.src_loc.clone());
            return false;
        }
        let ir_num = ir_num.unwrap();
        let start_loc = &self.ir_locs[*ir_num];

        let Some(val) = start_loc.addr_base.checked_add(start_loc.addr_offset) else {
            diags.err1(
                "EXEC_59",
                "__OUTPUT_ADDR: address overflow.",
                ir.src_loc.clone(),
            );
            return false;
        };
        *self.parms[out_parm_num].to_u64_mut() = val;
        true
    }

    fn iterate_builtin_version_string(&mut self, ir: &IR) -> bool {
        assert!(ir.operands.len() == 1);
        self.parms[ir.operands[0]] =
            ParameterValue::QuotedString(ConstBuiltins::get().brink_version_string.to_string());
        true
    }

    fn iterate_builtin_version_major(&mut self, ir: &IR) -> bool {
        assert!(ir.operands.len() == 1);
        *self.parms[ir.operands[0]].to_u64_mut() = ConstBuiltins::get().brink_version_major;
        true
    }

    fn iterate_builtin_version_minor(&mut self, ir: &IR) -> bool {
        assert!(ir.operands.len() == 1);
        *self.parms[ir.operands[0]].to_u64_mut() = ConstBuiltins::get().brink_version_minor;
        true
    }

    fn iterate_builtin_version_patch(&mut self, ir: &IR) -> bool {
        assert!(ir.operands.len() == 1);
        *self.parms[ir.operands[0]].to_u64_mut() = ConstBuiltins::get().brink_version_patch;
        true
    }

    /// Compute the transient current address.  This case is called when
    /// addr/addr_offset/sec_offset is called without an identifier.
    fn iterate_current_address(&mut self, ir: &IR, diags: &mut Diags, current: &Location) -> bool {
        self.trace(
            format!(
                "Engine::iterate_current_address: file_pos {}, off {}, sec {}",
                current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );
        assert!(ir.operands.len() == 1);
        let out_parm_num = ir.operands[0];
        let out_parm = &mut self.parms[out_parm_num];
        let out = out_parm.to_u64_mut();

        match ir.kind {
            IRKind::Addr => {
                let Some(val) = current.addr_base.checked_add(current.addr_offset) else {
                    diags.err1(
                        "EXEC_39",
                        "Absolute address (abs_base + off) overflow",
                        ir.src_loc.clone(),
                    );
                    return false;
                };
                *out = val;
            }
            IRKind::AddrOffset => {
                *out = current.addr_offset;
            }
            IRKind::SecOffset => {
                *out = current.sec_offset;
            }
            IRKind::FileOffset => {
                *out = current.file_offset;
            }
            bad => {
                panic!("Called iterate_current_address with bogus IR {:?}", bad);
            }
        }

        true
    }

    /// Compute the required number of bytes to align the current absolute location.
    /// We don't actually align anything yet, since that happens in a subsequent
    /// wr8 instruction.
    fn iterate_align(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &Location,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_align: file_pos {}, off {}, sec {}",
                current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );

        let num_operands = ir.operands.len();

        // The first parameter is the align amount
        // The optional second parameter is the pad value.  We don't care about
        // the pad value anymore, since that parameter pertains only to the wr8.
        // The final parameter is the result operand for the number
        // of bytes required to align.
        assert!(num_operands == 2 || num_operands == 3);
        let out_parm_num = if num_operands == 2 {
            ir.operands[1]
        } else {
            ir.operands[2]
        };

        let align_parm_num = ir.operands[0];
        let align_val = self.parms[align_parm_num].to_u64();

        if align_val == 0 {
            // Align 0 causes division by zero at checked_rem.
            let src_loc = irdb.parms[align_parm_num].src_loc.clone();
            diags.err1("EXEC_38", "Alignment amount cannot be zero", src_loc);
            return false;
        }

        let out_parm = &mut self.parms[out_parm_num];
        let out = out_parm.to_u64_mut();

        let Some(abs_val) = current.addr_base.checked_add(current.addr_offset) else {
            diags.err1(
                "EXEC_42",
                "Absolute address (abs_base + off) overflow",
                ir.src_loc.clone(),
            );
            return false;
        };

        let remainder = abs_val.checked_rem(align_val).unwrap();

        *out = if remainder == 0 {
            0 // we're already aligned, no pad bytes needed
        } else {
            align_val - remainder
        };

        debug!("Engine::iterate_align: alignment amount is {}", *out);
        true
    }

    /// Compute the required number of bytes to pad the current section to the specified size.
    /// We don't actually pad anything yet, since that happens in a subsequent
    /// wr8 instruction.
    /// This function covers set_sec_offset and set_addr_offset.
    fn iterate_set(
        &mut self,
        ir: &IR,
        _irdb: &IRDb,
        diags: &mut Diags,
        current: &Location,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_set: {:?}: file_pos {}, off {}, sec {}",
                ir.kind, current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );

        let num_operands = ir.operands.len();

        // The first parameter is the pad amount
        // The optional second parameter is the pad value.  We don't care about
        // the pad value anymore, since that parameter pertains only to the wr8.
        // The final parameter is the result operand for the number
        // of bytes required to pad.
        assert!(num_operands == 2 || num_operands == 3);
        let out_parm_num = if num_operands == 2 {
            ir.operands[1]
        } else {
            ir.operands[2]
        };

        let set_parm_num = ir.operands[0];
        let set_val = self.parms[set_parm_num].to_u64();

        let out_parm = &mut self.parms[out_parm_num];
        let out = out_parm.to_u64_mut();

        let loc = match ir.kind {
            IRKind::SetAddrOffset => current.addr_offset,
            IRKind::SetSecOffset => current.sec_offset,
            IRKind::SetFileOffset => current.file_offset,
            bad => panic!("called iterate_set for IR {:?}", bad),
        };

        // The current location can never move backwards
        if set_val < loc {
            let msg = format!(
                "Set statement moves location counter backwards from {} to {}.",
                loc, set_val
            );
            diags.err1("EXEC_22", &msg, ir.src_loc.clone());
            return false;
        }

        *out = set_val - loc;

        debug!("Engine::iterate_set: {:?} set amount is {}", ir.kind, *out);
        true
    }

    /// Handle `set_addr(X)`: pure cursor rebase.
    /// Sets abs_base = X and resets off = 0.  No bytes are emitted.
    /// Backward rebase is valid (firmware load-address use case).
    fn iterate_set_addr(&mut self, ir: &IR, current: &mut Location) -> bool {
        let set_parm_num = ir.operands[0];
        let set_val = self.parms[set_parm_num].to_u64();

        self.trace(
            format!(
                "Engine::iterate_set_addr: abs_base {} -> {}, off reset to 0",
                current.addr_base, set_val
            )
            .as_str(),
        );

        let num_operands = ir.operands.len();
        assert!(num_operands == 2 || num_operands == 3);
        let out_parm_num = if num_operands == 2 {
            ir.operands[1]
        } else {
            ir.operands[2]
        };

        // Record that set_addr was called mid-section if sec_offset is non-zero.
        // This arms the warning for any subsequent set_sec_offset in this scope.
        if current.sec_offset != 0 {
            if let Some(frame) = self.scope_stack.last_mut() {
                frame.set_addr_seen = true;
            }
        }

        current.addr_base = set_val;
        current.addr_offset = 0;

        // No bytes to write; tell the consumer wr8 to emit 0 bytes.
        *self.parms[out_parm_num].to_u64_mut() = 0;

        true
    }

    /// Compute the transient address of the identifier.  This case is called when
    /// addr/addr_offset/sec_offset is called with an identifier.
    fn iterate_identifier_address(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &Location,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_identifier_address: file_pos {}, off {}, sec {}",
                current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );
        // addr/addr_offset/sec_offset take one optional input and produce one output.
        // We've already discarded surrounding () on the operand.
        assert!(ir.operands.len() == 2);
        let in_parm_num0 = ir.operands[0]; // identifier
        let out_parm_num = ir.operands[1];

        let name = self.parms[in_parm_num0].to_identifier().to_string();

        let out_parm = &mut self.parms[out_parm_num];
        let out = out_parm.to_u64_mut();

        // We've already verified that the section identifier exists,
        // but unless the section actually got used in the output,
        // then we won't find location info for it.
        let ir_num = irdb.addressed_locs.get(&name);
        if ir_num.is_none() {
            let msg = format!(
                "Address of section or label '{}' not reachable in output.",
                name
            );
            diags.err1("EXEC_11", &msg, ir.src_loc.clone());
            return false;
        }
        let ir_num = ir_num.unwrap();
        let start_loc = &self.ir_locs[*ir_num];
        match ir.kind {
            IRKind::Addr => {
                let Some(val) = start_loc.addr_base.checked_add(start_loc.addr_offset) else {
                    diags.err1(
                        "EXEC_44",
                        "Absolute address (abs_base + off) overflow for identifier",
                        ir.src_loc.clone(),
                    );
                    return false;
                };
                *out = val;
            }
            IRKind::AddrOffset => {
                *out = start_loc.addr_offset;
            }
            IRKind::SecOffset => {
                *out = start_loc.sec_offset;
            }
            IRKind::FileOffset => {
                *out = start_loc.file_offset;
            }
            bad => {
                panic!("Called iterate_identifier_address with bogus IR {:?}", bad);
            }
        }

        true
    }

    fn iterate_address(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        current: &Location,
    ) -> bool {
        self.trace(
            format!(
                "Engine::iterate_address: file_pos {}, off {}, sec {}",
                current.file_offset, current.addr_offset, current.sec_offset
            )
            .as_str(),
        );
        // addr/addr_offset/sec_offset take one optional input and produce one output.
        // We've already discarded surrounding () on the operand.
        let num_operands = ir.operands.len();

        match num_operands {
            1 => self.iterate_current_address(ir, diags, current),
            2 => self.iterate_identifier_address(ir, irdb, diags, current),
            bad => panic!("Wrong number of IR operands = {}!", bad),
        }
    }

    /// On section entry, save all parent cursor state that the child may modify.
    fn iterate_section_start(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        _diags: &mut Diags,
        current: &mut Location,
    ) -> bool {
        let sec_name = irdb.get_opnd_as_identifier(ir, 0).to_string();
        self.scope_stack.push(ScopeFrame {
            sec_offset: current.sec_offset,
            addr_base: current.addr_base,
            addr_offset: current.addr_offset,
            sec_name,
            set_addr_seen: false,
        });
        self.trace(
            format!(
                "Engine::iterate_section_start: file_pos {}, sec {}",
                current.file_offset, current.sec_offset
            )
            .as_str(),
        );
        current.sec_offset = 0;

        true
    }

    /// On section exit, restore parent cursor state and advance the parent's
    /// offsets by the child's byte count.
    fn iterate_section_end(
        &mut self,
        ir: &IR,
        irdb: &IRDb,
        _diags: &mut Diags,
        current: &mut Location,
    ) -> bool {
        let child_size = current.sec_offset;
        let frame = self.scope_stack.pop().unwrap();
        self.trace(
            format!(
                "Engine::iterate_section_end: '{}', child_size {}, file_pos {}",
                irdb.get_opnd_as_identifier(ir, 0),
                child_size,
                current.file_offset
            )
            .as_str(),
        );
        current.sec_offset = frame.sec_offset + child_size;
        current.addr_base = frame.addr_base;
        current.addr_offset = frame.addr_offset + child_size;

        true
    }

    pub fn new(
        irdb: &IRDb,
        ext_registry: &ExtensionRegistry,
        diags: &mut Diags,
        abs_start: usize,
    ) -> anyhow::Result<Self> {
        // The first iterate loop may access any IR location, so initialize all
        // ir_locs locations to zero.
        let ir_locs = vec![
            Location {
                file_offset: 0,
                addr_offset: 0,
                sec_offset: 0,
                addr_base: irdb.start_addr
            };
            irdb.ir_vec.len()
        ];

        let mut engine = Engine {
            parms: Vec::new(),
            ir_locs,
            scope_stack: Vec::new(),
            warned_lids: HashSet::new(),
            start_addr: irdb.start_addr,
            wr_dispatches: Vec::new(),
            label_dispatches: Vec::new(),
        };
        engine.trace("Engine::new:");

        // Initialize parameters from the IR operands.
        engine.parms.reserve(irdb.parms.len());
        for opnd in &irdb.parms {
            engine.parms.push(opnd.clone_val());
        }

        let result = engine.iterate(irdb, ext_registry, diags, abs_start);
        if !result {
            anyhow::bail!("Engine construction failed.");
        }

        engine.build_dispatches(irdb);
        engine.trace("Engine::new: EXIT");
        Ok(engine)
    }

    /// Scans `ir_vec` and the stable `ir_locs` to build `wr_dispatches` and
    /// `label_dispatches`.  Called once after iterate converges.
    ///
    /// `ir_locs[i]` holds the file offset *before* IR `i` executes, so:
    ///   - a `SectionStart` at index `i` begins at `ir_locs[i].file_pos`
    ///   - the matching `SectionEnd` at index `j` ends at `ir_locs[j].file_pos`
    ///   - section size = `ir_locs[j].file_pos - ir_locs[i].file_pos`
    ///
    /// A section written N times produces N `WrDispatch` entries in output order.
    fn build_dispatches(&mut self, irdb: &IRDb) {
        // Stack of (section_name, SectionStart IR index) for matching ends.
        let mut stack: Vec<(String, usize)> = Vec::new();
        for (i, ir) in irdb.ir_vec.iter().enumerate() {
            match ir.kind {
                IRKind::SectionStart => {
                    let name = irdb.get_opnd_as_identifier(ir, 0).to_string();
                    stack.push((name, i));
                }
                IRKind::SectionEnd => {
                    let (name, start_idx) = stack.pop().expect("Unmatched SectionEnd in ir_vec");
                    let start_loc = &self.ir_locs[start_idx];
                    let file_start = start_loc.file_offset;
                    let file_end = self.ir_locs[i].file_offset;
                    let addr_offset = start_loc.addr_offset;
                    let addr = start_loc.addr_base.saturating_add(addr_offset);
                    self.wr_dispatches.push(WrDispatch {
                        name,
                        file_offset: file_start,
                        addr_offset,
                        addr,
                        size: file_end - file_start,
                    });
                }
                IRKind::Label => {
                    let name = irdb.get_opnd_as_identifier(ir, 0).to_string();
                    let loc = &self.ir_locs[i];
                    let file_offset = loc.file_offset;
                    let addr_offset = loc.addr_offset;
                    let addr = loc.addr_base.saturating_add(addr_offset);
                    self.label_dispatches.push(LabelDispatch {
                        name,
                        file_offset,
                        addr_offset,
                        addr,
                    });
                }
                _ => {}
            }
        }
    }

    pub fn dump_locations(&self) {
        for (idx, loc) in self.ir_locs.iter().enumerate() {
            debug!("{}: {:?}", idx, loc);
        }
    }

    /// Repeatedly executes the IR until all location-dependent values
    /// (addresses, alignments, section sizes) stabilize.  Each pass walks the
    /// full `irdb.ir_vec` in order, updating `self.ir_locs` with the image and
    /// section offset recorded after each instruction.  Because an alignment or
    /// `sizeof` expression may change the size of an earlier region on a later
    /// pass, iteration continues until two consecutive passes produce identical
    /// location vectors.  `abs_start` sets the base image address for the first
    /// section.  Returns `false` and emits diagnostics if any instruction fails
    /// validation or execution during the loop.
    pub fn iterate(
        &mut self,
        irdb: &IRDb,
        ext_registry: &ExtensionRegistry,
        diags: &mut Diags,
        abs_start: usize,
    ) -> bool {
        self.trace(format!("Engine::iterate: abs_start = {}", abs_start).as_str());
        let mut result = true;
        let mut old_locations = Vec::new();
        let mut stable = false;
        let mut iter_count = 0;
        while result && !stable {
            self.trace(format!("Engine::iterate: Iteration count {}", iter_count).as_str());
            iter_count += 1;
            let mut current = Location {
                file_offset: 0,
                addr_offset: 0,
                sec_offset: 0,
                addr_base: self.start_addr,
            };

            // make sure we exited as many sections as we entered on each iteration
            assert!(self.scope_stack.is_empty());

            for (lid, ir) in irdb.ir_vec.iter().enumerate() {
                debug!(
                    "Engine::iterate on lid {} at file_pos {}",
                    lid, current.file_offset
                );
                // record our location after each IR
                self.ir_locs[lid] = current.clone();
                let operation = ir.kind;
                result &= match operation {
                    // Arithmetic with two operands in, one out
                    IRKind::Add
                    | IRKind::Subtract
                    | IRKind::RightShift
                    | IRKind::LeftShift
                    | IRKind::BitAnd
                    | IRKind::LogicalAnd
                    | IRKind::BitOr
                    | IRKind::LogicalOr
                    | IRKind::Multiply
                    | IRKind::Divide
                    | IRKind::Modulo
                    | IRKind::DoubleEq
                    | IRKind::GEq
                    | IRKind::LEq
                    | IRKind::Gt
                    | IRKind::Lt
                    | IRKind::NEq => self.iterate_arithmetic(ir, irdb, operation, &current, diags),
                    IRKind::ToI64 | IRKind::ToU64 => {
                        self.iterate_type_conversion(ir, irdb, operation, &current, diags)
                    }
                    IRKind::Sizeof => self.iterate_sizeof(ir, irdb, diags, &current),
                    IRKind::SizeofExt => self.iterate_sizeof_ext(ir, diags, ext_registry),
                    IRKind::BuiltinOutputSize => self.iterate_output_size(ir, irdb, diags),
                    IRKind::BuiltinOutputAddr => self.iterate_output_addr(ir, irdb, diags),
                    IRKind::BuiltinVersionString => self.iterate_builtin_version_string(ir),
                    IRKind::BuiltinVersionMajor => self.iterate_builtin_version_major(ir),
                    IRKind::BuiltinVersionMinor => self.iterate_builtin_version_minor(ir),
                    IRKind::BuiltinVersionPatch => self.iterate_builtin_version_patch(ir),

                    // Unlike print, we have to iterate on the string write operation since
                    // the size of the string affects the size of the output image.
                    IRKind::Addr | IRKind::AddrOffset | IRKind::SecOffset | IRKind::FileOffset => {
                        self.iterate_address(ir, irdb, diags, &current)
                    }
                    IRKind::Wrs => self.iterate_wrs(ir, irdb, diags, &mut current),
                    IRKind::SectionStart => {
                        self.iterate_section_start(ir, irdb, diags, &mut current)
                    }
                    IRKind::SectionEnd => self.iterate_section_end(ir, irdb, diags, &mut current),

                    IRKind::Wr(_) => self.iterate_wrx(ir, irdb, diags, &mut current),
                    IRKind::WrExt => {
                        self.iterate_wrext(ir, irdb, &mut current, ext_registry, diags)
                    }
                    IRKind::Align => self.iterate_align(ir, irdb, diags, &current),
                    IRKind::SetSecOffset | IRKind::SetAddrOffset | IRKind::SetFileOffset => {
                        if ir.kind != IRKind::SetFileOffset {
                            if let Some(true) = self.scope_stack.last().map(|f| &f.set_addr_seen) {
                                if self.warned_lids.insert((lid, "EXEC_54")) {
                                    let cmd = if ir.kind == IRKind::SetSecOffset {
                                        "set_sec_offset"
                                    } else {
                                        "set_addr_offset"
                                    };
                                    let msg = format!(
                                        "[EXEC_54] Warning: '{}' follows 'set_addr' in the same \
                                         section scope.  '{}' pads to a sec/addr_offset value, \
                                         not to an address.  Consider 'set_addr_offset' after \
                                         'set_addr' to pad relative to the address anchor.",
                                        cmd, cmd
                                    );
                                    diags.warn1("EXEC_54", &msg, ir.src_loc.clone());
                                }
                            }
                        }
                        self.iterate_set(ir, irdb, diags, &current)
                    }
                    IRKind::SetAddr => self.iterate_set_addr(ir, &mut current),

                    IRKind::Wrf => self.iterate_wrf(ir, irdb, diags, &mut current),

                    // The following IR types are evaluated only at execute time.
                    // Nothing to do during iteration.
                    IRKind::Const
                    | IRKind::Eq
                    | IRKind::Label
                    | IRKind::Assert
                    | IRKind::Print
                    | IRKind::I64
                    | IRKind::U64
                    | IRKind::ExtensionCall
                    | IRKind::ExtensionCallRanged
                    | IRKind::ExtensionCallSection
                    // if/else IR only lives in const_ir_vec; never reaches the engine.
                    | IRKind::ConstDeclare
                    | IRKind::IfBegin
                    | IRKind::ElseBegin
                    | IRKind::IfEnd
                    | IRKind::BareAssign => true,
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
        let opnd = &self.parms[opnd_num];
        let ir_opnd = &irdb.parms[opnd_num];
        if opnd.data_type() == DataType::U64 {
            let val = opnd.to_u64();
            let msg = format!("Operand has value {}", val);
            let primary_code_ref = ir_opnd.src_loc.clone();
            diags.note1("EXEC_8", &msg, primary_code_ref);
        }
    }

    /// Display additional diagnostic if the assertion occurred for an
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

    /// Validation phase: evaluates every `assert` in the IR against the
    /// completed image.  Runs after both `execute_core_operations` and
    /// `execute_extensions`, so extension output is fully committed.
    fn execute_validation(&self, irdb: &IRDb, diags: &mut Diags) -> Result<()> {
        self.trace("Engine::execute_validation:");
        let mut error_count = 0;
        for ir in &irdb.ir_vec {
            if ir.kind == IRKind::Assert && self.execute_assert(ir, irdb, diags).is_err() {
                error_count += 1;
                if error_count > 10 {
                    break;
                }
            }
        }
        if error_count > 0 {
            return Err(anyhow!("Error detected"));
        }
        Ok(())
    }

    fn execute_assert(&self, ir: &IR, irdb: &IRDb, diags: &mut Diags) -> Result<()> {
        self.trace("Engine::execute_assert:");
        let mut result = Ok(());
        let opnd_num = ir.operands[0];
        self.trace(format!("engine::execute_assert: checking operand {}", opnd_num).as_str());
        let parm = &self.parms[opnd_num];
        if !parm.to_bool() {
            // assert failed
            let msg = "Assert expression failed".to_string();
            diags.err1("EXEC_2", &msg, ir.src_loc.clone());

            // If the boolean the assertion failed on is an output of an operation,
            // then backtrack to print information about that operation.  To backtrack
            // we get the Option<src_lid> for the assert.
            let src_lid = irdb.get_operand_ir_lid(opnd_num);
            self.assert_info(src_lid, irdb, diags);
            result = Err(anyhow!("Assert failed"));
        }

        result
    }

    /// Execute the print statement.
    /// If the diags noprint option is true, suppress printing.
    fn execute_print(&self, ir: &IR, irdb: &IRDb, diags: &mut Diags, _file: &File) -> Result<()> {
        self.trace("Engine::execute_print:");
        if diags.noprint {
            debug!("Suppressing print statements.");
            return Ok(());
        }

        let xstr_opt = self.evaluate_string_expr(ir, irdb, diags);
        if xstr_opt.is_none() {
            let msg = "Evaluating string expression failed.".to_string();
            diags.err1("EXEC_16", &msg, ir.src_loc.clone());
            return Err(anyhow!("Wrs failed"));
        }

        let xstr = xstr_opt.unwrap();
        print!("{}", xstr);
        Ok(())
    }

    fn execute_wrs(
        &self,
        written_ranges: &mut WrittenRanges,
        lid: usize,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
    ) -> Result<()> {
        self.trace("Engine::execute_wrs:");
        let xstr_opt = self.evaluate_string_expr(ir, irdb, diags);
        if xstr_opt.is_none() {
            let msg = "Evaluating string expression failed.".to_string();
            diags.err1("EXEC_15", &msg, ir.src_loc.clone());
            return Err(anyhow!("Wrs failed"));
        }

        let xstr = xstr_opt.unwrap();
        let size = xstr.len() as u64;
        let loc = &self.ir_locs[lid];
        let addr = loc.addr_base + loc.addr_offset;
        if !Self::check_and_record_range(written_ranges, addr, size, ir.src_loc.clone(), diags) {
            return Err(anyhow!("Address overwrite detected"));
        }

        let bufs = xstr.as_bytes();
        // the map_error lambda just converts io::error to a std::error
        let result = file.write_all(bufs).map_err(|err| err.into());
        if result.is_err() {
            let msg = "Writing string failed".to_string();
            diags.err1("EXEC_3", &msg, ir.src_loc.clone());
        }

        result
    }

    fn execute_wrf(
        &self,
        written_ranges: &mut WrittenRanges,
        lid: usize,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
    ) -> Result<()> {
        self.trace("Engine::execute_wrf:");

        let path = self.parms[ir.operands[0]].to_str().to_owned();

        // we already verified this is a legit file path,
        // so unwrap is ok.
        let file_size = irdb.files.get(path.as_str()).unwrap().size;

        let loc = &self.ir_locs[lid];
        let addr = loc.addr_base + loc.addr_offset;
        if !Self::check_and_record_range(written_ranges, addr, file_size, ir.src_loc.clone(), diags)
        {
            return Err(anyhow!("Address overwrite detected"));
        }

        let mut source_file = match File::open(path.as_str()) {
            Ok(f) => f,
            Err(err) => {
                let msg = format!(
                    "Opening file '{path}' failed with OS error '{:?}'.",
                    err.raw_os_error()
                );
                diags.err1("EXEC_33", &msg, ir.src_loc.clone());
                return Err(anyhow!(err));
            }
        };

        // read/write in 64K chunks
        // TODO don't hardcode this number
        let mut buf = [0u8; 0x10000];
        let mut total_bytes = 0;
        loop {
            // the map_error lambda just converts io::error to a std::error
            let bytes_read = match source_file.read(&mut buf) {
                Ok(bytes) => bytes,
                Err(err) => {
                    let msg = format!(
                        "Reading file '{path}' failed with OS error '{:?}'.",
                        err.raw_os_error()
                    );
                    diags.err1("EXEC_34", &msg, ir.src_loc.clone());
                    return Err(anyhow!(err));
                }
            };

            total_bytes += bytes_read;
            let write_result = file
                .write_all(&buf[0..bytes_read])
                .map_err(|err| err.into());
            if write_result.is_err() {
                let msg = "Writing buffer failed".to_string();
                diags.err1("EXEC_35", &msg, ir.src_loc.clone());
                return write_result;
            }

            if bytes_read < buf.len() {
                break; // source file is exhausted, nothing more to write
            }
        }

        assert!(total_bytes as u64 == file_size);

        Ok(())
    }

    fn execute_wrx(
        &self,
        written_ranges: &mut WrittenRanges,
        lid: usize,
        ir: &IR,
        _irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
    ) -> Result<()> {
        self.trace(format!("Engine::execute_wrx: {:?}", ir.kind).as_str());
        let byte_size = get_wrx_byte_width(ir);

        let opnd_num = ir.operands[0];
        self.trace(format!("engine::execute_wrx: checking operand {}", opnd_num).as_str());
        let parm = &self.parms[opnd_num];

        // Extract bytes as little-endian.  One a big-endian machine, the LSB will
        // bit the highest address location, which is wrong since we're writing
        // from the lowest address.
        let buf = match parm.data_type() {
            DataType::Integer | DataType::I64 => {
                let val = parm.to_i64();
                val.to_le_bytes()
            }
            DataType::U64 => {
                let val = parm.to_u64();
                val.to_le_bytes()
            }
            bad => {
                panic!("Unexpected parameter type {:?} in execute_wrx", bad);
            }
        };

        let mut repeat_count = 1;

        if ir.operands.len() == 2 {
            // Yes, we have a repeat count
            // We already validated the operands in IRDB.
            let repeat_opnd_num = ir.operands[1];
            let op = &self.parms[repeat_opnd_num];
            repeat_count = op.to_u64();
        }

        self.trace(format!("Repeat count = {}", repeat_count).as_str());
        let total_size = (byte_size as u64) * repeat_count;
        let loc = &self.ir_locs[lid];
        let addr = loc.addr_base + loc.addr_offset;
        if !Self::check_and_record_range(
            written_ranges,
            addr,
            total_size,
            ir.src_loc.clone(),
            diags,
        ) {
            return Err(anyhow!("Address overwrite detected"));
        }

        // The map_error lambda just converts io::error to a std::error
        // Write only the number of bytes required for the width of the wrx
        while repeat_count > 0 {
            let result = file.write_all(&buf[0..byte_size]).map_err(|err| err.into());
            if result.is_err() {
                let msg = format!("{:?} failed", ir.kind);
                diags.err1("EXEC_18", &msg, ir.src_loc.clone());
                return result;
            }
            repeat_count -= 1;
        }

        Ok(())
    }

    /// Records `[start, start+size)` as written.  If the new range overlaps any
    /// previously recorded range, emits EXEC_55 and returns `false`.  The caller
    /// should propagate the error; the overlapping range is still recorded so
    /// that further independent overlaps can also be reported.
    fn check_and_record_range(
        written_ranges: &mut WrittenRanges,
        start: u64,
        size: u64,
        src_loc: SourceSpan,
        diags: &mut Diags,
    ) -> bool {
        if size == 0 {
            return true;
        }
        let end = start + size; // safe: callers already checked for overflow

        // Check against the range that starts at or just before `start`.
        let overlap = written_ranges
            .range(..=start)
            .next_back()
            .filter(|(_, (prev_end, _))| *prev_end > start)
            .map(|(&prev_start, (prev_end, prev_loc))| (prev_start, *prev_end, prev_loc.clone()))
            .or_else(|| {
                // Also check the range that starts strictly after `start` —
                // it overlaps if its start is less than our end.
                written_ranges
                    .range(start + 1..)
                    .next()
                    .filter(|&(&next_start, _)| next_start < end)
                    .map(|(&next_start, (next_end, next_loc))| {
                        (next_start, *next_end, next_loc.clone())
                    })
            });

        written_ranges.insert(start, (end, src_loc.clone()));

        if let Some((prev_start, prev_end, prev_loc)) = overlap {
            let msg = format!(
                "Address range {:#x}..{:#x} overlaps previously written range {:#x}..{:#x}",
                start, end, prev_start, prev_end,
            );
            diags.err2("EXEC_55", &msg, src_loc, prev_loc);
            return false;
        }

        true
    }

    /// Performs the generate and validation passes over the IR.
    /// Called once after `iterate` reached a stable location assignment.
    ///
    /// Phase order matches the README specification:
    /// 1. Generate — writes output bytes (`Wr`, `Wrs`, `Wrf`, `WrExt` pre-pad, `Print`).
    /// 2. Extensions — patches extension output into the memory-mapped file.
    /// 3. Validation — evaluates all `assert` statements against the completed image.
    ///
    /// Returns `Err` and emits diagnostics if any write or assertion fails.
    pub fn execute(
        &self,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
        ext_registry: &ExtensionRegistry,
    ) -> Result<()> {
        self.trace("Engine::execute:");
        let mut written_ranges = BTreeMap::new();

        self.execute_core_operations(&mut written_ranges, irdb, diags, file, ext_registry)?;
        self.execute_extensions(irdb, diags, file, ext_registry)?;
        self.execute_validation(irdb, diags)?;

        Ok(())
    }

    /// Generate phase: writes all output bytes and print side-effects.
    /// Assert statements are intentionally skipped here — they run in
    /// `execute_validation` after the image is fully written.
    fn execute_core_operations(
        &self,
        written_ranges: &mut WrittenRanges,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
        ext_registry: &ExtensionRegistry,
    ) -> Result<()> {
        self.trace("Engine::execute_core_operations:");
        let mut result;
        let mut error_count = 0;
        for (lid, ir) in irdb.ir_vec.iter().enumerate() {
            result = match ir.kind {
                IRKind::Wr(_) => self.execute_wrx(written_ranges, lid, ir, irdb, diags, file),
                IRKind::Print => self.execute_print(ir, irdb, diags, file),
                IRKind::Wrs => self.execute_wrs(written_ranges, lid, ir, irdb, diags, file),
                IRKind::Wrf => self.execute_wrf(written_ranges, lid, ir, irdb, diags, file),
                // Assert runs in the validation phase, after all bytes are written.
                IRKind::Assert => Ok(()),
                // the rest of these operations are computed during iteration
                IRKind::SetSecOffset
                | IRKind::SetAddrOffset
                | IRKind::SetAddr
                | IRKind::SetFileOffset
                | IRKind::Align
                | IRKind::Addr
                | IRKind::Const
                | IRKind::AddrOffset
                | IRKind::SecOffset
                | IRKind::FileOffset
                | IRKind::Label
                | IRKind::Sizeof
                | IRKind::SizeofExt
                | IRKind::BuiltinOutputSize
                | IRKind::BuiltinOutputAddr
                | IRKind::BuiltinVersionString
                | IRKind::BuiltinVersionMajor
                | IRKind::BuiltinVersionMinor
                | IRKind::BuiltinVersionPatch
                | IRKind::ToI64
                | IRKind::ToU64
                | IRKind::Eq
                | IRKind::NEq
                | IRKind::GEq
                | IRKind::LEq
                | IRKind::Gt
                | IRKind::Lt
                | IRKind::DoubleEq
                | IRKind::I64
                | IRKind::U64
                | IRKind::BitAnd
                | IRKind::LogicalAnd
                | IRKind::BitOr
                | IRKind::LogicalOr
                | IRKind::Multiply
                | IRKind::Modulo
                | IRKind::Divide
                | IRKind::Add
                | IRKind::Subtract
                | IRKind::SectionStart
                | IRKind::SectionEnd
                | IRKind::LeftShift
                | IRKind::RightShift
                | IRKind::ExtensionCall
                | IRKind::ExtensionCallRanged
                | IRKind::ExtensionCallSection
                // if/else IR only lives in const_ir_vec; never reaches the engine.
                | IRKind::ConstDeclare
                | IRKind::IfBegin
                | IRKind::ElseBegin
                | IRKind::IfEnd
                | IRKind::BareAssign => Ok(()),
                IRKind::WrExt => {
                    // We must emit empty zeroed bytes during the physical serial write loop
                    // to guarantee that the output file expands to encompass the extension's
                    // final layout boundaries before memory mapping executes.
                    let opnd_idx = ir.operands[0];
                    let opnd = &irdb.parms[opnd_idx];
                    if let Some(prod_ir_idx) = opnd.is_output_of() {
                        let prod_ir = &irdb.ir_vec[prod_ir_idx];
                        if matches!(
                            prod_ir.kind,
                            IRKind::ExtensionCall
                                | IRKind::ExtensionCallRanged
                                | IRKind::ExtensionCallSection
                        ) {
                            let ext_name_opnd = &irdb.parms[prod_ir.operands[0]];
                            let ext_name = ext_name_opnd.val.to_identifier();
                            if let Some(entry) = ext_registry.get(ext_name) {
                                let buf = vec![0u8; entry.cached_size];
                                if let Err(err) = file.write_all(&buf) {
                                    return Err(anyhow::anyhow!(
                                        "Failed to pre-pad space for extension '{}': {}",
                                        ext_name, err
                                    ));
                                }
                            }
                        }
                    }
                    Ok(())
                }
            };

            if result.is_err() {
                error_count += 1;
                if error_count > 10 {
                    // todo parameterize max 10 errors
                    break;
                }
            }
        }

        if error_count > 0 {
            return Err(anyhow!("Error detected"));
        }
        Ok(())
    }

    /// Isolates and evaluates all extension calls.
    /// Builds a topological dependency DAG to execute extensions in the correct order.
    fn execute_extensions(
        &self,
        irdb: &IRDb,
        diags: &mut Diags,
        file: &mut File,
        ext_registry: &ExtensionRegistry,
    ) -> Result<()> {
        self.trace("Engine::execute_extensions:");

        // Scope extraction: we isolate ONLY the extension calls
        // decoupled from the core pipeline logic.
        let mut extension_nodes = Vec::new();
        for (idx, ir) in irdb.ir_vec.iter().enumerate() {
            if matches!(
                ir.kind,
                IRKind::ExtensionCall | IRKind::ExtensionCallRanged | IRKind::ExtensionCallSection
            ) {
                extension_nodes.push(idx);
            }
        }

        if extension_nodes.is_empty() {
            return Ok(());
        }

        use memmap2::MmapOptions;

        // We memory-map the output file to allow extensions zero-copy access to the fully generated image,
        // and allow zero-copy patching of the extension's execution output without re-reading from disk.
        // We synchronize the file first to ensure the OS sees all written data/padding before mapping.
        if let Err(e) = file.sync_all() {
            return Err(anyhow!(
                "Failed to sync output file before memory mapping: {}",
                e
            ));
        }

        // This is the only bit of `unsafe code in brink.
        let mut mmap = match unsafe { MmapOptions::new().map_mut(&*file) } {
            Ok(m) => m,
            Err(e) => return Err(anyhow!("Failed to memory map output file: {}", e)),
        };

        // Sequentially execute extensions and patch the output file.
        let mut error_count = 0;
        for &idx in &extension_nodes {
            let ir = &irdb.ir_vec[idx];
            let name = self.parms[ir.operands[0]].to_identifier();
            let out_idx = *ir.operands.last().unwrap();

            // Find the consumer of this ExtensionCall's output
            let mut consumer_ir = None;
            let mut consumer_idx = 0;
            for (c_idx, c_ir) in irdb.ir_vec.iter().enumerate() {
                if c_idx != idx && c_ir.operands.contains(&out_idx) {
                    consumer_ir = Some(c_ir);
                    consumer_idx = c_idx;
                    break;
                }
            }

            let Some(c_ir) = consumer_ir else {
                diags.err1(
                    "EXEC_45",
                    &format!("Extension '{}' output not consumed", name),
                    ir.src_loc.clone(),
                );
                error_count += 1;
                continue;
            };

            let Some(entry) = ext_registry.get(name) else {
                unreachable!("Extension '{}' not found in registry", name);
            };

            let byte_width = match c_ir.kind {
                IRKind::WrExt => entry.cached_size,
                _ => {
                    diags.err1(
                        "EXEC_46",
                        &format!("Extension '{}' must be consumed by a generic `wr` statement. Fixed-size writes like `wr32` are prohibited.", name),
                        ir.src_loc.clone(),
                    );
                    error_count += 1;
                    continue;
                }
            };

            // Build (img_slice, args) according to the call form.
            //
            // ExtensionCall (form 1):
            //   operands = [name, arg0..., output]
            //   No image access; args are operands[1..last].
            //
            // ExtensionCallRanged (form 2):
            //   operands = [name, range_start, range_length, arg0..., output]
            //   img_slice = mmap[range_start..range_start+range_length]
            //   args are operands[3..last].
            //
            // ExtensionCallSection (form 3):
            //   operands = [name, section_id, arg0..., output]
            //   Engine resolves (file_offset, size) from wr_dispatches.
            //   args are operands[2..last].
            let last = ir.operands.len() - 1;
            let (img_slice_range, arg_operand_range) = match ir.kind {
                IRKind::ExtensionCall => (0..0, 1..last),
                IRKind::ExtensionCallRanged => {
                    let start = self.parms[ir.operands[1]].to_u64() as usize;
                    let length = self.parms[ir.operands[2]].to_u64() as usize;
                    (start..start + length, 3..last)
                }
                IRKind::ExtensionCallSection => {
                    let sec_name = self.parms[ir.operands[1]].to_identifier();
                    let count = self
                        .wr_dispatches
                        .iter()
                        .filter(|d| d.name == sec_name)
                        .count();
                    if count > 1 {
                        diags.err1(
                            "EXEC_56",
                            &format!(
                                "Section '{}' appears {} times in the output; \
                                 the section-name form is ambiguous. Wrap with unique \
                                 section name(s) or use the explicit-range form such as: \
                                 `wr {}(<file_offset>, <length>)` to specify which occurrence.",
                                sec_name, count, name,
                            ),
                            ir.src_loc.clone(),
                        );
                        error_count += 1;
                        continue;
                    }
                    let dispatch = self.wr_dispatches.iter().find(|d| d.name == sec_name);
                    let Some(d) = dispatch else {
                        return Err(anyhow!(
                            "Extension '{}': section '{}' not found in dispatch table. \
                             This is a compiler bug.",
                            name,
                            sec_name
                        ));
                    };
                    let start = d.file_offset as usize;
                    (start..start + d.size as usize, 2..last)
                }
                _ => unreachable!(),
            };

            let args: Vec<u64> = ir.operands[arg_operand_range]
                .iter()
                .map(|&i| self.parms[i].to_u64())
                .collect();

            let mut out_buffer = vec![0u8; byte_width];

            let exec_result = match &entry.extension {
                RegisteredExtension::Basic(e) => e.execute(&args, &mut out_buffer),
                RegisteredExtension::Ranged(e) => {
                    e.execute(&args, &mmap[img_slice_range], &mut out_buffer)
                }
            };

            if let Err(e) = exec_result {
                let msg = format!("Extension '{}' execution failed: {}", name, e);
                diags.err1("EXEC_47", &msg, ir.src_loc.clone());
                return Err(anyhow!(msg));
            }

            // Patch the file at the exact image offset where the consumer instruction evaluated.
            let loc = &self.ir_locs[consumer_idx];
            let abs_offset = loc.file_offset as usize;

            if abs_offset + byte_width > mmap.len() {
                return Err(anyhow!(
                    "Extension bounded write exceeds file bounds implicitly. This is a severe compiler bug."
                ));
            }

            // Zero-copy assignment back into OS memory map.
            mmap[abs_offset..abs_offset + byte_width].copy_from_slice(&out_buffer);
        }

        if error_count > 0 {
            return Err(anyhow!("Error detected in extension execution"));
        }

        if let Err(e) = mmap.flush() {
            return Err(anyhow!("Failed to flush memory-mapped file patches: {}", e));
        }

        Ok(())
    }
}
