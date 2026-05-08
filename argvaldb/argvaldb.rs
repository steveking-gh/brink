use diags::Diags;
use ir::{DataType, IROperand, ParameterValue};

pub struct ParmValDb {
    pub parms: Vec<ParameterValue>,
}

impl ParmValDb {
    pub fn new(parms: Vec<ParameterValue>) -> Self {
        Self { parms }
    }
}

/// Build a string from a sequence of typed operand values.
/// `parms` and `ir_parms` are global operand tables shared across all IR instructions.
/// Each entry has a unique integer ID assigned at IR construction time.
/// `indices` is `ir.operands`: the list of IDs that belong to this one instruction,
/// used to select the relevant entries from the global tables.
pub fn evaluate_string_expr(
    parms: &[ParameterValue],
    ir_parms: &[IROperand],
    indices: &[usize],
    diags: &mut Diags,
) -> Option<String> {
    let mut result = true;
    let mut xstr = String::new();
    for &op_num in indices {
        let op = &parms[op_num];
        match op.data_type() {
            DataType::QuotedString => xstr.push_str(op.to_str()),
            DataType::U64 => xstr.push_str(&format!("{:#X}", op.to_u64())),
            DataType::Integer | DataType::I64 => xstr.push_str(&format!("{}", op.to_i64())),
            bad => {
                let msg = format!("Cannot stringify type '{:?}'", bad);
                diags.err1("ERR_138", &msg, ir_parms[op_num].src_loc.clone());
                result = false;
            }
        }
    }
    if result { Some(xstr) } else { None }
}
