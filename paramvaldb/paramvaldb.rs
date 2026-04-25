use ir::ParameterValue;

pub struct ParmValDb {
    pub parms: Vec<ParameterValue>,
}

impl ParmValDb {
    pub fn new(parms: Vec<ParameterValue>) -> Self {
        Self { parms }
    }
}
