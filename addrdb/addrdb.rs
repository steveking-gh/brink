// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};
use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;
use ast::{Ast,AstDb};
use lineardb::LinearDb;
use std::collections::HashMap;

struct AddrInfo {
    abs_addr: usize,
    size: usize,
    nid: NodeId,
    lid: usize,
}

/*****************************************************************************
 * AddrDb
 * The AddrDb contains a map of the logical address and size of all items with a
 * size in the linear DB. The key is the AST NodeID, the value is the size.
 *****************************************************************************/
pub struct AddrDb {
    addr_vec : Vec<AddrInfo>,
}

impl<'toks> AddrDb {

    pub fn new(linear_db: &LinearDb, _diags: &mut Diags, ast: &'toks Ast,
               _ast_db: &'toks AstDb, abs_start: usize) -> AddrDb {

        debug!("AddrDb::new: >>>> ENTER at {}", abs_start);

        let mut addr_db = AddrDb { addr_vec: Vec::new() };

        addr_db
    }

    /// Dump the DB for debug
    pub fn dump(&self) {
        for ainfo in &self.addr_vec {
            debug!("AddrDb: nid {} is {} bytes at absolute address {}",
                    ainfo.nid, ainfo.size, ainfo.abs_addr);
        }
    }
}
