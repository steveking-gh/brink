// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::collections::HashMap;
use std::vec::Vec;
use std::{io,fs};
use logos::{Logos};
use indextree::{Arena,NodeId};
extern crate clap;
use clap::{Arg, App};

// codespan crate provide error reporting help
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

pub type Span = std::ops::Range<usize>;

#[derive(Logos, Debug, Clone, PartialEq)]
pub enum LexToken {
    #[token("section")] Section,
    #[token("wr")] Wr,
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

/**
 * Abstract Syntax Tree
 * This structure contains the AST created from the raw lexical
 * tokens.  The lifetime of this struct is the same as the tokens.
 */
pub struct Ast<'toks> {
    arena: Arena<usize>,
    ltv: &'toks[TokenInfo<'toks>],
    root: NodeId,
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
            debug!("Parsing token {}: {:?}", &mut tok_num, tinfo);
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
            debug!("Parsing token {}: {:?}", *tok_num, tinfo);
            match tinfo.tok {
                // For now, we only support writing strings in a section.
                LexToken::Wr => {
                    if !self.parse_wr(tok_num, parent, ctxt) {
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

    fn parse_wr(&mut self, tok_num : &mut usize, parent : NodeId,
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
            self.err_expected_after(ctxt, 4, "Expected a quoted string after 'wr'", tok_num);
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
        debug!("parse_wr success");
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
        let node = self.arena.new_node(*tok_num);
        parent.append(node, &mut self.arena);
        *tok_num += 1;
    }

    fn get_tok(&self, nid: NodeId) -> &'toks TokenInfo {
        let tok_num = *self.arena[nid].get();
        &self.ltv[tok_num]
    }

    fn dump_r(&self, nid: NodeId, depth: usize) {
        print!("AST: ");
        println!("{}{}", " ".repeat(depth * 4), self.get_tok(nid).slice());
        let children = nid.children(&self.arena);
        for child_nid in children {
            self.dump_r(child_nid, depth+1);
        }
    }

    /**
     * Recursively dumps the AST to the console.
     */
    pub fn dump(&self) {
        println!();
        let children = self.root.children(&self.arena);
        for child_nid in children {
            self.dump_r(child_nid, 0);
        }
        println!();
    }
}

trait ActionItem {
    fn size(&self) -> usize;
    fn get_tinfo(&self) -> &TokenInfo;
    fn iterate(&self, ast: &Ast, all_db: &mut AllDB) -> bool;
    fn get_ast_nid(&self) -> NodeId;
}

/*******************************
 * Section
 ******************************/
struct Section<'toks> {
    tinfo: &'toks TokenInfo<'toks>,
    nid: NodeId,
    size: usize,
    actions: Vec<Box<dyn ActionItem>>,
}

impl<'toks> Section<'toks> {
    pub fn new(ast: &'toks Ast, nid: NodeId) -> Section<'toks> {
        Section { tinfo: ast.get_tok(nid), nid, size: 0, actions: Vec::new() }
    }
}

impl<'toks> ActionItem for Section<'toks> {
    fn size(&self) -> usize { self.size }
    fn get_tinfo(&self) -> &TokenInfo { &self.tinfo }
    fn iterate(&self, ast: &Ast, all_db: &mut AllDB) -> bool { true }
    fn get_ast_nid(&self) -> NodeId { self.nid }
}

/*******************************
 * Output
 ******************************/
 struct Output<'toks> {
    tinfo: &'toks TokenInfo<'toks>,
    nid: NodeId,
    size: usize,
    sec_nid: NodeId,
    sec_str: &'toks str,
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
        Output { tinfo: ast.get_tok(nid), nid, size: 0,
                sec_nid, sec_str}
    }

    pub fn get_sec_name(&self) -> &'toks str { self.sec_str }
}

impl<'toks> ActionItem for Output<'toks> {
    fn size(&self) -> usize { self.size }
    fn get_tinfo(&self) -> &TokenInfo { &self.tinfo }
    fn iterate(&self, ast: &Ast, all_db: &mut AllDB) -> bool { true }
    fn get_ast_nid(&self) -> NodeId { self.nid }
}

/*******************************
 * AllDB
 ******************************/
 struct AllDB<'toks> {
    section_db: HashMap<&'toks str, Section<'toks>>,
}

impl<'toks> AllDB<'toks> {
    pub fn new() -> AllDB<'toks> {
        AllDB { section_db: HashMap::new() }
    }
}

/// Processes a section in the AST
/// ctxt: the system context
/// sec_nid: section node ID in the AST
fn inventory_sections<'toks>(ctxt: &mut Context, sec_nid: NodeId,
                          ast: &'toks Ast, all_db: &mut AllDB<'toks>) -> bool {
    // nid points to 'section'
    // the first child of section is the section identifier
    // AST processing guarantees this exists, so unwrap
    let mut children = sec_nid.children(&ast.arena);
    let sec_id_nid = children.next().unwrap();
    let sec_id_tinfo = ast.get_tok(sec_id_nid);
    let sec_id_str = sec_id_tinfo.slice();
    if all_db.section_db.contains_key(sec_id_str) {
        // error, duplicate section names
        // We know the section exists, so unwrap is fine.
        let orig_section = all_db.section_db.get(sec_id_str).unwrap();
        let orig_tinfo = orig_section.tinfo;
        let diag = Diagnostic::error()
                .with_code("ERR_9")
                .with_message(format!("Duplicate section name '{}'", sec_id_str))
                .with_labels(vec![Label::primary((), sec_id_tinfo.span()),
                                Label::secondary((), orig_tinfo.span())]);
        ctxt.diags.emit(&diag);
        return false;
    }
    all_db.section_db.insert(sec_id_str, Section::new(&ast,sec_nid));
    true
}

fn inventory_outputs<'toks>(_ctxt: &mut Context, output_nid: NodeId,
                         ast: &'toks Ast, output_vec: &mut Vec<Output<'toks>>) -> bool {
    // nid points to 'output'
    // don't bother with semantic error checking yet.
    // The lexer already did basic checking
    output_vec.push(Output::new(&ast, output_nid));
    true
}

fn process_output<'toks>(_ctxt: &mut Context, output: &mut Output,
    ast: &'toks Ast, all_db: &mut AllDB<'toks>) -> bool {
    info!("Processing output {}", output.get_sec_name());

    true
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

    let mut all_db = AllDB::new();

    let mut result = true;

    let mut output_vec = Vec::new();

    // Populate the database of critical structures.
    for nid in ast.root.children(&ast.arena) {
        let tinfo = ast.get_tok(nid);
        result = match tinfo.tok {
            LexToken::Section => inventory_sections(&mut ctxt, nid, &ast, &mut all_db),
            LexToken::Output => inventory_outputs(&mut ctxt, nid, &ast, &mut output_vec),
            _ => { true }
        };

        if !result { break;}
    }

    if output_vec.is_empty() {
        let diag = Diagnostic::warning()
                .with_code("WARN_10")
                .with_message("No output statement, nothing to do.");
        ctxt.diags.emit(&diag);
    }

    for mut op in output_vec {
        process_output(&mut ctxt, &mut op, &ast, &mut all_db);
    }

    result
}

// Logging
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

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
