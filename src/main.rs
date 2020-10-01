// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use std::io;
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
    _Root_,

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

pub fn parse<'toks>(ltv: &'toks[TokenInfo], tn: &mut usize, ctxt: &mut Context) -> Arena<usize> {
    let mut arena = Arena::new();

    // For uniformity, we want a root node as a parent for recursion
    // The LexToken index of the root should not get confused with an
    // actual index into ltv, so use MAX.
    let mut root = arena.new_node(usize::MAX);
    let tinfo = &ltv[*tn];
    println!("Parsing token {}: {:?}", *tn, tinfo);
    match tinfo.tok {
        LexToken::Section => parse_section(&mut arena, ltv, tn, &mut root, ctxt),
        _ => println!("Error"),
    }

    arena
}

pub fn parse_section<'toks>(arena : &mut Arena<usize>, ltv : &'toks[TokenInfo],
                            tn : &mut usize, parent : &mut NodeId, ctxt: &mut Context) {

    // Add the section keyword as a child of the parent
    // All content in the section are children of the section node
    let mut node = arena.new_node(*tn);
    parent.append(node, arena);

    // Advance the token number past 'section'
    *tn += 1;

    // After a section declaration, an identifier is expected
    let tinfo = &ltv[*tn];
    if let LexToken::Identifier = tinfo.tok {
        parse_identifier(arena, tn, &mut node);
    } else {
        let diag = Diagnostic::error()
            .with_code("E001")
            .with_message(format!("Expected an identifier after 'section', instead found '{}'", tinfo.slice()))
            .with_labels(vec![Label::primary((), tinfo.span()),
                              Label::secondary((), ltv[*tn-1].span())]);

        ctxt.diags.emit(&diag);
    }
}

pub fn parse_identifier(arena : &mut Arena<usize>,
                        tn : &mut usize, parent : &mut NodeId) {

    // add the identifier as a child of the parent
    let node = arena.new_node(*tn);
    parent.append(node, arena);

    // Identifiers are leaf nodes.  Just advance the token index and return.
    *tn += 1;
}

/// Entry point for all processing on the input source file
/// name: The name of the file
/// fstr: A string containing the file
pub fn process(name: &str, fstr: &str) {

    let mut ctxt = Context {
        diags: Diags::new(name,fstr),
    };

    let mut tv = Vec::new();
    let mut lex = LexToken::lexer(fstr);
    while let Some(t) = lex.next() {
        tv.push(TokenInfo{tok: t, s:lex.slice(), loc: lex.span()});
    }


    let mut tok_index = 0;
    let arena = parse(&tv, &mut tok_index, &mut ctxt);
    for node in arena.iter() {
        if *node.get() == usize::MAX {
            continue; // skip the fake root node
        }
        println!("Node = {} for token {:?}", *node.get(), tv[*node.get()] );
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
                .help("The input specification file.")
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

            /*
    let result = fs::read_to_string(in_file_name);
    if result.is_err() {
        let e = result.err().unwrap();
        eprintln!("Unable to read file '{}'\nError: {}", in_file_name, e);
        std::process::exit(-1);
    }
    let in_file = result.unwrap();
    */

    let in_file = "section {} section foo{/*stu\nff*/92};// line \"quote\" comment\n section bar {0x56};\nsection foo {\"w\\\"o\nw\"}\n\nsection baz {0}";

    process(&in_file_name, &in_file);
}
