// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use std::{io,fs};
use logos::{Logos};
use indextree::{Arena,NodeId};
extern crate clap;
use clap::{Arg, App};

// codespan crate provide error reporting help
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::files::Files;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};

pub type Span = std::ops::Range<usize>;

#[derive(Logos, Debug, Clone, PartialEq)]
pub enum LexToken {
    #[token("section")] Section,
    #[token("wr")] Wr,
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

    fn _line_number(&self, byte_index: usize) -> usize {
        self.source_map.line_index((),byte_index).unwrap() + 1
    }
}


/// Context for most functions.  This struct is just a handy bundle
/// of other structs used to keep function parameter lists under
/// control.
pub struct Context<'a> {
    /// Diagnostic interface, generally for error messages
    diags: Diags<'a>,
}

pub fn parse<'toks>(arena : &mut Arena<usize>, ltv: &'toks[TokenInfo],
                    ctxt: &mut Context) -> bool {
    let total_toks = ltv.len();
    if total_toks == 0 {
        return true;
    }

    let mut tok_num = 0;
    while tok_num < total_toks {
        let tinfo = &ltv[tok_num];
        debug!("Parsing token {}: {:?}", &mut tok_num, tinfo);
        match tinfo.tok {
            LexToken::Section => {
                if !parse_section(arena, ltv, &mut tok_num, None, ctxt) {
                    return false;
                }
            }
            _ => { return false; },
        }
    }
    true
}

pub fn parse_section<'toks>(arena : &mut Arena<usize>, ltv : &'toks[TokenInfo],
                            tok_num : &mut usize, parent : Option<NodeId>,
                            ctxt: &mut Context) -> bool {

    // Add the section keyword as a child of the parent
    // All content in the section are children of the section node
    let node = arena.new_node(*tok_num);

    // If the parent exists, attach the child node
    if let Some(p) = parent {
        p.append(node, arena);
    }

    // Advance the token number past 'section'
    *tok_num += 1;

    // After a section declaration, an identifier is expected
    let tinfo = &ltv[*tok_num];
    if let LexToken::Identifier = tinfo.tok {
        parse_leaf(arena, tok_num, node);
    } else {
        let diag = Diagnostic::error()
            .with_code("E001")
            .with_message(format!("Expected an identifier after 'section', instead found '{}'", tinfo.slice()))
            .with_labels(vec![Label::primary((), tinfo.span()),
                              Label::secondary((), ltv[*tok_num-1].span())]);
        ctxt.diags.emit(&diag);
        return false;
    }

    // After a section identifier, open brace
    let tinfo = &ltv[*tok_num];
    if let LexToken::OpenBrace = tinfo.tok {
        parse_leaf(arena, tok_num, node);
    } else {
        let diag = Diagnostic::error()
            .with_code("E002")
            .with_message(format!("Expected '{{' after identifier, instead found '{}'", tinfo.slice()))
            .with_labels(vec![Label::primary((), tinfo.span()),
                              Label::secondary((), ltv[*tok_num-1].span())]);
        ctxt.diags.emit(&diag);
        return false;
    }

    parse_section_contents(arena, ltv, tok_num, node, ctxt);

    true
}

pub fn parse_section_contents<'toks>(arena : &mut Arena<usize>, ltv : &'toks[TokenInfo],
                              tok_num : &mut usize, parent : NodeId, ctxt: &mut Context) -> bool {


    let total_toks = ltv.len();
    if total_toks == 0 {
        return true;
    }

    while *tok_num < total_toks {
        let tinfo = &ltv[*tok_num];
        debug!("Parsing token {}: {:?}", *tok_num, tinfo);
        match tinfo.tok {
            // For now, we only support writing strings in a section.
            LexToken::Wr => parse_leaf(arena, tok_num, parent),
            LexToken::CloseBrace => {
                // When we find a close brace, we're done with section content
                parse_leaf(arena, tok_num, parent);
                return true;
            }
            _ => {
                let diag = Diagnostic::error()
                    .with_code("E003")
                    .with_message(format!("Invalid expression in section '{}'", tinfo.slice()))
                    .with_labels(vec![Label::primary((), tinfo.span())]);
                ctxt.diags.emit(&diag);
                return false;
            }
        }
    }
    true
}

/**
 * Adds the token as a child of teh parent and advances
 * the token index.
 */
pub fn parse_leaf(arena : &mut Arena<usize>,
        tok_num : &mut usize, parent : NodeId) {
    let node = arena.new_node(*tok_num);
    parent.append(node, arena);
    *tok_num += 1;
}

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(name: &str, fstr: &str) {
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

    let mut arena = Arena::new();
    let success = parse(&mut arena, &tv, &mut ctxt);
    println!("Parsing {}", if success {"succeeded"} else {"failed"});
    for (node_num, tok_num) in arena.iter().enumerate() {
        println!("Node {} is token #{} = {}", node_num, *tok_num.get(), tv[*tok_num.get()].s);
    }

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

    process(&in_file_name, &in_file);
}
