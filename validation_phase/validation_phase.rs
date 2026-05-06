// Assert validation phase.
//
// ValidationPhase evaluates all assert instructions after layout completes
// and before the execution phase writes bytes.  Assert expressions reference
// only layout-computed values in ParmValDb -- no byte access to the output
// file exists in the current IR.
//
// NOTE: If extension return values are implemented, keep ValidationPhase
// ordered after execute_extensions so extension output flows into ParmValDb
// before assert evaluation.

use anyhow::{Result, anyhow};
use diags::Diags;
use ir::{DataType, IR, IRKind};
use irdb::IRDb;
use argvaldb::ParmValDb;

#[allow(unused_imports)]
use tracing::trace;

pub struct ValidationPhase {}

impl ValidationPhase {
    pub fn validate(argvaldb: &ParmValDb, irdb: &IRDb, diags: &mut Diags) -> Result<()> {
        trace!("ValidationPhase::validate:");
        let mut error_count = 0;
        for ir in &irdb.ir_vec {
            if ir.kind == IRKind::Assert
                && Self::execute_assert(argvaldb, ir, irdb, diags).is_err()
            {
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

    fn execute_assert(
        argvaldb: &ParmValDb,
        ir: &IR,
        irdb: &IRDb,
        diags: &mut Diags,
    ) -> Result<()> {
        trace!("ValidationPhase::execute_assert:");
        let opnd_num = ir.operands[0];
        trace!(
            "{}",
            format!("validation::execute_assert: checking operand {}", opnd_num).as_str()
        );
        let parm = &argvaldb.parms[opnd_num];
        if !parm
            .to_bool()
            .expect("assert operand must be numeric; IRDb type check failed")
        {
            let msg = "Assert expression failed".to_string();
            diags.err1("ERR_126", &msg, ir.src_loc.clone());

            // Backtrack to print the operand values of the failing comparison.
            let src_lid = irdb.get_operand_ir_lid(opnd_num);
            Self::assert_info(argvaldb, src_lid, irdb, diags);
            return Err(anyhow!("Assert failed"));
        }

        Ok(())
    }

    fn assert_info(
        argvaldb: &ParmValDb,
        src_lid: Option<usize>,
        irdb: &IRDb,
        diags: &mut Diags,
    ) {
        let Some(src_lid) = src_lid else {
            return;
        };
        let operation = irdb.ir_vec.get(src_lid).unwrap();
        let num_operands = operation.operands.len();
        for (idx, opnd) in operation.operands.iter().enumerate() {
            if idx < num_operands - 1 {
                Self::assert_info_operand(argvaldb, *opnd, irdb, diags);
            }
        }
    }

    fn assert_info_operand(
        argvaldb: &ParmValDb,
        opnd_num: usize,
        irdb: &IRDb,
        diags: &mut Diags,
    ) {
        let opnd = &argvaldb.parms[opnd_num];
        let ir_opnd = &irdb.parms[opnd_num];
        if opnd.data_type() == DataType::U64 {
            let val = opnd.to_u64();
            let msg = format!("Operand has value {}", val);
            let primary_code_ref = ir_opnd.src_loc.clone();
            diags.note1("ERR_132", &msg, primary_code_ref);
        }
    }
}
