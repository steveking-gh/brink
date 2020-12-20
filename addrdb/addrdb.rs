// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};
use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;
use ast::{Ast,AstDb};
use lineardb::LinearDb;
use std::collections::HashMap;

trait AddrInfo {
    fn get_abs_addr(&self) -> usize;
    fn get_nid(&self) -> NodeId;
    fn get_size(&self) -> usize;
}

/*****************************************************************************
 * AddrDb
 * The AddrDb contains a map of the logical address and size of all items with a
 * size in the linear DB. The key is the AST NodeID, the value is the size.
 *****************************************************************************/
pub struct AddrDb<'toks> {
    addrs : HashMap<NodeId, Box<dyn AddrInfo + 'toks>>,
}

impl<'toks> AddrDb<'toks> {

    pub fn new(linear_db: &LinearDb, _diags: &mut Diags, ast: &'toks Ast,
               _ast_db: &'toks AstDb, abs_start: usize) -> AddrDb<'toks> {

        debug!("AddrDb::new: >>>> ENTER for output nid: {} at {}", linear_db.output_nid,
                abs_start);
        let mut addrs : HashMap<NodeId, Box<dyn AddrInfo + 'toks>> = HashMap::new();

        AddrDb { addrs }
    }

    /// Dump the DB for debug
    pub fn dump(&self) {
        for (_nid, ainfo) in &self.addrs {
            debug!("AddrDb: nid {} is {} bytes at absolute address {}",
                    ainfo.get_nid(), ainfo.get_size(), ainfo.get_abs_addr());
        }
    }
}
