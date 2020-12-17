// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};
use indextree::{NodeId};
pub type Span = std::ops::Range<usize>;
use diags::Diags;
use ast::{Ast,AstDb};
use lineardb::LinearDb;

trait AddrInfo {
    fn set_abs_addr(&mut self, abs: usize);
    fn get_abs_addr(&self) -> usize;
    fn get_nid(&self) -> NodeId;
    fn get_size(&self) -> usize;
    fn get_type_str(&self) -> &'static str;
}

struct WrsAddrInfo {
    abs_addr: usize,
    nid: NodeId,
    str_size: usize,
}

impl<'toks> WrsAddrInfo {
    pub fn new(abs_addr: usize, nid: NodeId, ast: &'toks Ast) -> WrsAddrInfo {
        debug!("WrsAddrInfo::new: >>>> ENTER for nid {} at {}", nid, abs_addr);
        let strout = ast.get_child_str(nid, 0).trim_matches('\"');
        debug!("WrsAddrInfo::new: output string is {}", strout);
        let str_size = strout.len();
        debug!("WrsAddrInfo::new: <<<< EXIT for nid {}", nid);
        WrsAddrInfo{ abs_addr, nid, str_size}
    }
}

impl<'toks> AddrInfo for WrsAddrInfo {
    fn set_abs_addr(&mut self, abs: usize) { self.abs_addr = abs; }
    fn get_abs_addr(&self) -> usize { self.abs_addr}
    fn get_nid(&self) -> NodeId { self.nid}
    fn get_size(&self) -> usize { self.str_size }
    fn get_type_str(&self) -> &'static str {
        "wrs"
    }
}

/*****************************************************************************
 * AddrDb
 * The AddrDb contains a map of the logical size in bytes of all items with a
 * size in the AST. The key is the AST NodeID, the value is the size.
 *****************************************************************************/
pub struct AddrDb<'toks> {
    addrs : Vec<Box<dyn AddrInfo + 'toks>>,
}

impl<'toks> AddrDb<'toks> {

    pub fn new(linear_db: &LinearDb, _diags: &mut Diags, ast: &'toks Ast,
               _ast_db: &'toks AstDb, abs_start: usize) -> AddrDb<'toks> {

        debug!("AddrDb::new: >>>> ENTER for output nid: {} at {}", linear_db.output_nid,
                abs_start);
        let mut addrs : Vec<Box<dyn AddrInfo + 'toks>> = Vec::new();

        // First pass to build sizes
        let mut start = abs_start;
        let mut new_size = 0;
        for &nid in &linear_db.nidvec {
            let tinfo = ast.get_tinfo(nid);
            match tinfo.tok {
                ast::LexToken::Wrs => {
                    // TODO we don't need a box here! A normal struct will do fine.
                    let wrsa = Box::new(WrsAddrInfo::new(start, nid, ast));
                    let sz = wrsa.get_size();
                    start += sz;
                    new_size += sz;
                    addrs.push(wrsa);
                },
                _ => () // trivial zero size token like ';'.
            };
        }

        let mut old_size = new_size;
        let mut iteration = 1;
        // Iterate until the size of the section stops changing.
        loop {
            new_size = 0;
            for ainfo in &addrs {
                debug!("AddrDb::new: Iterating for {} at nid {}",
                        ainfo.get_type_str(), ainfo.get_nid());
                let sz = ainfo.get_size();
                start += sz;
                new_size += sz;
            }

            if old_size == new_size {
                break;
            }

            debug!("AddrDb::new: Size for iteration {} is {}", iteration, new_size);
            old_size = new_size;
            iteration += 1;
        }

        debug!("AddrDb::new: <<<< EXIT with size {}", new_size);
        AddrDb { addrs }
    }

    /// Dump the DB for debug
    pub fn dump(&self) {
        for a in &self.addrs {
            debug!("AddrDb: nid {} is {} bytes at absolute address {}",
                    a.get_nid(), a.get_size(), a.get_abs_addr());
        }
    }
}
