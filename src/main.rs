// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use std::{io,fs};
use logos::{Logos};
use indextree::NodeId;
extern crate clap;
use clap::{Arg, App};

// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

// codespan crate provide error reporting help
use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

pub type Span = std::ops::Range<usize>;

#[derive(Logos, Debug, Clone, PartialEq)]
pub enum LexToken {
    #[token("section")] Section,
    #[token("wrs")] Wrs,
    #[token("output")] Output,
    #[token("{")] OpenBrace,
    #[token("}")] CloseBrace,
    #[token(";")] Semicolon,
    #[regex("[_a-zA-Z][0-9a-zA-Z_]*")] Identifier,
    #[regex("0x[0-9a-fA-F]+|[1-9][0-9]*|0")] Int,

    // Not only is \ special in strings and must be escaped, but also special in
    // regex.  We use raw string here to avoid having the escape the \ for the
    // string itself. The \\ in this raw string are escape \ for the regex
    // engine underneath.
    #[regex(r#""(\\"|\\.|[^"])*""#)] QuotedString,

    // These are 'stripped' from the input
    #[regex(r#"/\*([^*]|\*[^/])+\*/"#, logos::skip)] // block comments
    #[regex(r#"//[^\r\n]*(\r\n|\n)?"#, logos::skip)] // line comments
    #[regex(r#"[ \t\n\f]+"#, logos::skip)]           // whitespace
    #[error]
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo<'toks> {
    tok : LexToken,
    loc : Span,
    s : &'toks str,
}

impl<'toks> TokenInfo<'toks> {
    pub fn span(&self) -> Span { self.loc.clone() }
    pub fn slice(&self) -> &str { &self.s }
}


struct Diags<'a> {
    writer: StandardStream,
    source_map: SimpleFile<&'a str, &'a str>,
    config: codespan_reporting::term::Config,
}

impl<'a> Diags<'a> {
    fn new(name: &'a str, fstr: &'a str) -> Self {
        Self {
            writer: StandardStream::stderr(ColorChoice::Always),
            source_map: SimpleFile::new(name,fstr),
            config: codespan_reporting::term::Config::default(),
        }
    }

    /// Writes the diagnostic to the terminal and returns a BErr
    /// with the diagnostic code
    fn emit(&self, diag: &Diagnostic<()>) {
        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, diag);
    }
}


/// Context for most functions.  This struct is just a handy bundle
/// of other structs used to keep function parameter lists under
/// control.
pub struct Context<'a> {
    /// Diagnostic interface, generally for error messages
    diags: Diags<'a>,
}

#[macro_use]
mod ast {
    use indextree::{Arena,NodeId};
    use super::TokenInfo;
    use super::Context;
    use super::LexToken;
    use codespan_reporting::diagnostic::{Diagnostic, Label};
    use std::collections::HashMap;

    #[allow(unused_imports)]
    use super::{error, warn, info, debug, trace};

    /**
     * Abstract Syntax Tree
     * This structure contains the AST created from the raw lexical
     * tokens.  The lifetime of this struct is the same as the tokens.
     */
    pub struct Ast<'toks> {
        pub arena: Arena<usize>,
        pub ltv: &'toks[TokenInfo<'toks>],
        pub root: NodeId,
    }

    impl<'toks> Ast<'toks> {
        pub fn new(ltv: &'toks[TokenInfo<'toks>]) -> Self {
            let mut a = Arena::new();
            let root = a.new_node(usize::MAX);
            Self { arena: a, ltv, root }
        }

        pub fn parse(&mut self, ctxt: &mut Context) -> bool {
            let toks_end = self.ltv.len();
            let mut tok_num = 0;
            while tok_num < toks_end {
                let tinfo = &self.ltv[tok_num];
                debug!("Ast::parse: Parsing token {}: {:?}", &mut tok_num, tinfo);
                match tinfo.tok {
                    LexToken::Section => {
                        if !self.parse_section(&mut tok_num, self.root, ctxt) {
                            return false;
                        }
                    },
                    LexToken::Output => {
                        if !self.parse_output(&mut tok_num, self.root, ctxt) {
                            return false;
                        }
                    },
                    _ => { return false; },
                }
            }
        true
        }

        fn err_expected_after(&self, ctxt: &mut Context, code: u32, msg: &str, tok_num: &usize) {
            let diag = Diagnostic::error()
                    .with_code(format!("ERR_{}", code))
                    .with_message(format!("{}, but found '{}'", msg, self.ltv[*tok_num].slice()))
                    .with_labels(vec![Label::primary((), self.ltv[*tok_num].span()),
                                    Label::secondary((), self.ltv[*tok_num-1].span())]);
            ctxt.diags.emit(&diag);
        }

        fn err_invalid_expression(&self, ctxt: &mut Context, code: u32, tok_num: &usize) {
            let diag = Diagnostic::error()
                    .with_code(format!("ERR_{}", code))
                    .with_message(format!("Invalid expression '{}'", self.ltv[*tok_num].slice()))
                    .with_labels(vec![Label::primary((), self.ltv[*tok_num].span())]);
            ctxt.diags.emit(&diag);
        }

        fn parse_section(&mut self, tok_num : &mut usize, parent : NodeId,
                        ctxt: &mut Context) -> bool {

            // Add the section keyword as a child of the parent and advance
            let node = self.arena.new_node(*tok_num);
            parent.append(node, &mut self.arena);
            *tok_num += 1;

            // After a section declaration, an identifier is expected
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::Identifier = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 1, "Expected an identifier after 'section'", tok_num);
                return false;
            }

            // After a section identifier, open brace
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::OpenBrace = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 2, "Expected {{ after identifier", tok_num);
                return false;
            }

            self.parse_section_contents(tok_num, node, ctxt);
            true
        }

        fn parse_section_contents(&mut self, tok_num : &mut usize, parent : NodeId,
                                            ctxt: &mut Context) -> bool {
            let toks_end = self.ltv.len();
            while *tok_num < toks_end {
                let tinfo = &self.ltv[*tok_num];
                match tinfo.tok {
                    // For now, we only support writing strings in a section.
                    LexToken::Wrs => {
                        if !self.parse_wrs(tok_num, parent, ctxt) {
                            return false;
                        }
                    }
                    LexToken::CloseBrace => {
                        // When we find a close brace, we're done with section content
                        self.parse_leaf(tok_num, parent);
                        return true;
                    }
                    _ => {
                        self.err_invalid_expression(ctxt, 3, tok_num);
                        return false;
                    }
                }
            }
            true
        }

        fn parse_wrs(&mut self, tok_num : &mut usize, parent : NodeId,
                    ctxt: &mut Context) -> bool {

            // Add the wr keyword as a child of the parent
            // Parameters of the wr are children of the wr node
            let node = self.arena.new_node(*tok_num);

            // wr must have a parent
            parent.append(node, &mut self.arena);

            // Advance the token number past 'wr'
            *tok_num += 1;

            // Next, a quoted string is expected
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::QuotedString = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 4, "Expected a quoted string after 'wrs'", tok_num);
                return false;
            }

            // Finally a semicolon
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::Semicolon = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 5, "Expected ';' after string", tok_num);
                return false;
            }
            debug!("parse_wrs success");
            true
        }

        fn parse_output(&mut self, tok_num : &mut usize, parent : NodeId,
                            ctxt: &mut Context) -> bool {

            // Add the output keyword as a child of the parent and advance
            let node = self.arena.new_node(*tok_num);
            parent.append(node, &mut self.arena);
            *tok_num += 1;

            // After a output declaration we expect a section identifier
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::Identifier = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 7, "Expected a section name after output", tok_num);
                return false;
            }

            // After the identifier, the file name as a quoted string
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::QuotedString = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 6, "Expected the file path as a quoted string after the section name", tok_num);
                return false;
            }

            // After the identifier, a semicolon
            let tinfo = &self.ltv[*tok_num];
            if let LexToken::Semicolon = tinfo.tok {
                self.parse_leaf(tok_num, node);
            } else {
                self.err_expected_after(ctxt, 8, "Expected ';' after identifier", tok_num);
                return false;
            }
            debug!("parse_output success");
            true
        }

        /**
         * Adds the token as a child of teh parent and advances
         * the token index.
         */
        fn parse_leaf(&mut self, tok_num : &mut usize, parent : NodeId) {
            let tinfo = &self.ltv[*tok_num]; // debug! only
            debug!("Ast::parse_leaf: Parsing token {}: {:?}", *tok_num, tinfo);
            let node = self.arena.new_node(*tok_num);
            parent.append(node, &mut self.arena);
            *tok_num += 1;
        }

        pub fn get_tok(&self, nid: NodeId) -> &'toks TokenInfo {
            let tok_num = *self.arena[nid].get();
            &self.ltv[tok_num]
        }

        fn dump_r(&self, nid: NodeId, depth: usize) {
            debug!("AST: {}: {}{}", nid, " ".repeat(depth * 4), self.get_tok(nid).slice());
            let children = nid.children(&self.arena);
            for child_nid in children {
                self.dump_r(child_nid, depth+1);
            }
        }

        /**
         * Recursively dumps the AST to the console.
         */
        pub fn dump(&self) {
            debug!("");
            let children = self.root.children(&self.arena);
            for child_nid in children {
                self.dump_r(child_nid, 0);
            }
            debug!("");
        }
    }

    /*******************************
     * Section
     ******************************/
    pub struct Section<'toks> {
        pub tinfo: &'toks TokenInfo<'toks>,
        pub nid: NodeId,
    }

    impl<'toks> Section<'toks> {
        pub fn new(ast: &'toks Ast, nid: NodeId) -> Section<'toks> {
            Section { tinfo: ast.get_tok(nid), nid }
        }
    }

    /*******************************
     * Output
     ******************************/
    pub struct Output<'toks> {
        pub tinfo: &'toks TokenInfo<'toks>,
        pub nid: NodeId,
        pub sec_nid: NodeId,
        pub sec_str: &'toks str,
    }

    impl<'toks> Output<'toks> {
        /// Create an new output object
        pub fn new(ast: &'toks Ast, nid: NodeId) -> Output<'toks> {
            let mut children = nid.children(&ast.arena);
            // the section name is the first child of the output
            // AST processing guarantees this exists.
            let sec_nid = children.next().unwrap();
            let sec_tinfo = ast.get_tok(sec_nid);
            let sec_str = sec_tinfo.slice();
            Output { tinfo: ast.get_tok(nid), nid, sec_nid, sec_str}
        }
    }

    /*****************************************************************************
     * AstDb
     * The AstDb contains a map of various items in the AST.
     * After construction, we never mutate this database.
     * The key is the AST NodeID, the value is the TokenInfo object.
     *****************************************************************************/
    pub struct AstDb<'toks> {
        pub sections: HashMap<&'toks str, Section<'toks>>,
        pub outputs: Vec<Output<'toks>>,
        //pub properties: HashMap<NodeId, NodeProperty>
    }

    impl<'toks> AstDb<'toks> {

        /// Processes a section in the AST
        /// ctxt: the system context
        fn record_section(ctxt: &mut Context, sec_nid: NodeId, ast: &'toks Ast,
                        sections: &mut HashMap<&'toks str, Section<'toks>> ) -> bool {
            debug!("AstDb::record_section: NodeId {}", sec_nid);

            // sec_nid points to 'section'
            // the first child of section is the section identifier
            // AST processing guarantees this exists, so unwrap
            let mut children = sec_nid.children(&ast.arena);
            let name_nid = children.next().unwrap();
            let sec_tinfo = ast.get_tok(name_nid);
            let sec_str = sec_tinfo.slice();
            if sections.contains_key(sec_str) {
                // error, duplicate section names
                // We know the section exists, so unwrap is fine.
                let orig_section = sections.get(sec_str).unwrap();
                let orig_tinfo = orig_section.tinfo;
                let diag = Diagnostic::error()
                        .with_code("ERR_9")
                        .with_message(format!("Duplicate section name '{}'", sec_str))
                        .with_labels(vec![Label::primary((), sec_tinfo.span()),
                                          Label::secondary((), orig_tinfo.span())]);
                ctxt.diags.emit(&diag);
                return false;
            }
            sections.insert(sec_str, Section::new(&ast,sec_nid));
            true
        }

        /**
         * Adds a new output to the vector of output structs.
         */
        fn record_output(_ctxt: &mut Context, nid: NodeId, ast: &'toks Ast,
                        outputs: &mut Vec<Output<'toks>>) -> bool {
            // nid points to 'output'
            // don't bother with semantic error checking yet.
            // The lexer already did basic checking
            debug!("AstDb::record_output: NodeId {}", nid);
            outputs.push(Output::new(&ast, nid));
            true
        }

        pub fn new(ctxt: &mut Context, ast: &'toks Ast) -> Option<AstDb<'toks>> {
            // Populate the AST database of critical structures.
            let mut result = true;

            let mut sections: HashMap<&'toks str, Section<'toks>> = HashMap::new();
            let mut outputs: Vec<Output<'toks>> = Vec::new();

            for nid in ast.root.children(&ast.arena) {
                let tinfo = ast.get_tok(nid);
                result = result && match tinfo.tok {
                    LexToken::Section => Self::record_section(ctxt, nid, &ast, &mut sections),
                    LexToken::Output => Self::record_output(ctxt, nid, &ast, &mut outputs),
                    _ => { true }
                };
            }

            if !result {
                return None;
            }

            Some(AstDb { sections, outputs })
        }
    }
}

trait ActionInfo {
    fn set_abs_addr(&mut self, abs: usize);
    fn get_abs_addr(&self) -> usize;
    fn get_nid(&self) -> NodeId;
    fn get_size(&self) -> usize;
    fn write(&self);
    fn get_type_str(&self) -> &'static str;
}

struct WrsActionInfo<'toks> {
    abs_addr: usize,
    nid: NodeId,
    str_nid: NodeId,
    str_size: usize,
    strout: &'toks str,
}

impl<'toks> WrsActionInfo<'toks> {
    pub fn new(abs_addr: usize, nid: NodeId, ast: &'toks Ast) -> WrsActionInfo<'toks> {
        debug!("WrsActionInfo::new: >>>> ENTER for nid {} at {}", nid, abs_addr);
        let mut children = nid.children(&ast.arena);
        let str_nid = children.next().unwrap();
        let str_tinfo = ast.get_tok(str_nid);
        // trim the leading and trailing quote characters
        let strout = str_tinfo.slice().trim_matches('\"');
        debug!("WrsActionInfo::new: output string at nid {} is {}", str_nid, strout);
        let str_size = strout.len();
        debug!("WrsActionInfo::new: <<<< EXIT for nid {}", nid);
        WrsActionInfo{ abs_addr, nid, str_nid, str_size, strout}
    }
}

impl<'toks> ActionInfo for WrsActionInfo<'toks> {
    fn set_abs_addr(&mut self, abs: usize) { self.abs_addr = abs; }
    fn get_abs_addr(&self) -> usize { self.abs_addr}
    fn get_nid(&self) -> NodeId { self.nid}
    fn get_size(&self) -> usize { self.str_size }
    fn write(&self) {} // temp for debug
    fn get_type_str(&self) -> &'static str {
        "wrs"
    }
}

/*****************************************************************************
 * ActionDB
 * The ActionDB contains a map of the logical size in bytes of all items with a
 * size in the AST. The key is the AST NodeID, the value is the size.
 *****************************************************************************/
struct ActionDB<'toks> {
    actions : Vec<Box<dyn ActionInfo + 'toks>>,
    output_nid: NodeId,
    file_name_str: &'toks str,
}

use ast::{Ast,AstDb};

impl<'toks> ActionDB<'toks> {

    /// Dump the DB for debug
    pub fn dump(&self) {
        for a in &self.actions {
            debug!("ActionDB: nid {} is {} bytes at absolute address {}",
                    a.get_nid(), a.get_size(), a.get_abs_addr());
        }
    }

    pub fn new(linear_db: &LinearDB, ctxt: &mut Context, ast: &'toks Ast,
               ast_db: &'toks AstDb, abs_start: usize) -> ActionDB<'toks> {

        debug!("ActionDB::new: >>>> ENTER for output nid: {} at {}", linear_db.output_nid,
                abs_start);
        let mut actions : Vec<Box<dyn ActionInfo + 'toks>> = Vec::new();
        let output_nid = linear_db.output_nid;


        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let mut children = output_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tok(sec_name_nid);
        let sec_str = sec_tinfo.slice();
        debug!("ActionDB::new: output section name is {}", sec_str);

        let file_name_nid = children.next().unwrap();
        let file_tinfo = ast.get_tok(file_name_nid);
        // strip the surrounding quote chars from the string
        let file_name_str = file_tinfo.slice().trim_matches('\"');
        debug!("ActionDB::new: output file name is {}", file_name_str);

        // Iterate until the size of the section stops changing.
        let mut start = abs_start;
        let mut new_size = 0;
        for &nid in &linear_db.nidvec {
            let tinfo = ast.get_tok(nid);
            match tinfo.tok {
                LexToken::Wrs => {
                    let wrsa = Box::new(WrsActionInfo::new(start, nid, ast));
                    let sz = wrsa.get_size();
                    start += sz;
                    new_size += sz;
                    actions.push(wrsa);
                },
                _ => () // trivial zero size token like ';'.
            };
        }

        let mut old_size = new_size;
        let mut iteration = 1;
        loop {
            new_size = 0;
            for ainfo in &actions {
                debug!("ActionDB:new: Iterating for {} at nid {}", ainfo.get_type_str(), ainfo.get_nid());
                let sz = ainfo.get_size();
                start += sz;
                new_size += sz;
            }

            if old_size == new_size {
                break;
            }
            debug!("ActionDB:new: Size for iteration {} is {}", iteration, new_size);
            old_size = new_size;
            iteration += 1;
        }

        debug!("ActionDB::new: <<<< EXIT with size {}", new_size);
        ActionDB { actions, output_nid, file_name_str }
    }

    pub fn write(&self) {
        use std::fs::File;
        use std::io::prelude::*;
    }
}

struct LinearDB {
    output_nid: NodeId,
    nidvec : Vec<NodeId>,
}

impl<'toks> LinearDB {
    /// Recursively record information about the children of an AST object.
    fn record_r(&mut self, parent_nid: NodeId, ctxt: &mut Context,
                            ast: &'toks Ast, ast_db: &AstDb) {

        debug!("LinearDB::record_children_info: >>>> ENTER for parent nid: {}",
                parent_nid);

        self.nidvec.push(parent_nid);
        let children = parent_nid.children(&ast.arena);
        for nid in children {
            self.record_r(nid, ctxt, ast, ast_db);
        }
        debug!("LinearDB::record_r: <<<< EXIT for nid: {}", parent_nid);
    }

    /// The ActionDB object must start with an output statement
    pub fn new(output_nid: NodeId, ctxt: &mut Context, ast: &'toks Ast,
               ast_db: &'toks AstDb) -> LinearDB {

        debug!("LinearDB::new: >>>> ENTER for output nid: {}", output_nid);
        let mut linear_db = LinearDB { output_nid, nidvec: Vec::new() };

        let mut children = output_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tok(sec_name_nid);
        let sec_str = sec_tinfo.slice();
        debug!("LinearDB::new: output section name is {}", sec_str);

        // Using the name of the section, use the AST database to get a reference
        // to the section object.  ast_db processing has already guaranteed
        // that the section name is legitimate, so unwrap().
        let section = ast_db.sections.get(sec_str).unwrap();
        let sec_nid = section.nid;

        linear_db.record_r(sec_nid, ctxt, ast, ast_db);

        debug!("LinearDB::new: <<<< EXIT for nid: {}", output_nid);
        linear_db
    }

    fn dump(&self) {
        debug!("LinearDB: Output NID {}", self.output_nid);
        for nid in &self.nidvec {
            debug!("LinearDB: {}", nid);
        }
    }
}

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(name: &str, fstr: &str) -> bool {
    info!("Processing {}", name);
    debug!("File contains: {}", fstr);

    let mut ctxt = Context {
        diags: Diags::new(name,fstr),
    };

    let mut tv = Vec::new();
    let mut lex = LexToken::lexer(fstr);
    while let Some(t) = lex.next() {
        tv.push(TokenInfo{tok: t, s:lex.slice(), loc: lex.span()});
    }

    let mut ast = Ast::new(tv.as_slice());
    let success = ast.parse(&mut ctxt);
    ast.dump();
    if !success {
        println!("AST construction failed");
        return false;
    }

    let ast_db_opt = AstDb::new(&mut ctxt, &ast);
    if ast_db_opt.is_none() {
        return false;
    }

    let ast_db = ast_db_opt.unwrap();

    if ast_db.outputs.is_empty() {
        let diag = Diagnostic::warning()
                .with_code("WARN_10")
                .with_message("No output statement, nothing to do.");
        ctxt.diags.emit(&diag);
    }

    // Take the reference to the ast_db to avoid a move due to the
    // implicit into_iter().
    // http://xion.io/post/code/rust-for-loop.html
    // https://stackoverflow.com/q/43036279/233981
    for outp in &ast_db.outputs {
        let linear_db = LinearDB::new(outp.nid, &mut ctxt, &ast, &ast_db);
        linear_db.dump();
        let action_db = ActionDB::new(&linear_db, &mut ctxt, &ast, &ast_db, 0);
        action_db.dump();



    }
    true
}

fn init_log(verbosity : u64) -> Result<(), fern::InitError>  {
    let mut base_cfg = fern::Dispatch::new();

    base_cfg = match verbosity {
        0 => base_cfg.level(log::LevelFilter::Error), // Quiet
        1 => base_cfg.level(log::LevelFilter::Warn),  // Default
        2 => base_cfg.level(log::LevelFilter::Info),  // -v
        3 => base_cfg.level(log::LevelFilter::Debug), // -v -v
        _4_or_more => base_cfg.level(log::LevelFilter::Trace), // -v -v -v
    };

    let stdout_cfg = fern::Dispatch::new()
            .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] {}",
                record.level(),
                message))
            })
            .chain(io::stdout());

    base_cfg.chain(stdout_cfg)
            .apply()?;
    Ok(())
}

fn main() {
    // clap processes args
    let args = App::new("roust")
            // See Cargo.toml for env! CARGO strings.
            .version(env!("CARGO_PKG_VERSION"))
            .author(env!("CARGO_PKG_AUTHORS"))
            .about(env!("CARGO_PKG_DESCRIPTION"))
            .arg(Arg::with_name("INPUT")
                .help("The input source file.")
                .required(true)
                .index(1))
            .arg(Arg::with_name("verbosity")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Sets the verbosity level. Use up to 4 times."))
            .arg(Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .help("Suppress non-error console output.  Overrides -v."))
            .get_matches();

    // Default verbosity
    let verbosity = if args.is_present("quiet") {
        0
    } else {
        1 + args.occurrences_of("verbosity")
    };

    init_log(verbosity).expect("Unknown error initializing logging.");

    info!("roust version {}", env!("CARGO_PKG_VERSION"));

    // Read the roust file into a string and pass to parser.
    // A bland error message here is fine since clap already
    // provides nice error messages.
    let in_file_name = args.value_of("INPUT")
            .expect("Strange input file argument error.");

    let result = fs::read_to_string(in_file_name);
    if result.is_err() {
        let e = result.err().unwrap();
        eprintln!("Unable to read file '{}'\nError: {}", in_file_name, e);
        std::process::exit(-1);
    }
    let in_file = result.unwrap();

    if !process(&in_file_name, &in_file) {
        std::process::exit(-1);
    }
}
