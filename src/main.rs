// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use logos::{Logos};

pub type Span = std::ops::Range<usize>;

#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo {
    loc : Span,
    s : String,
}

impl TokenInfo {
    pub fn span(&self) -> &Span { &self.loc }
    pub fn slice(&self) -> &str { &self.s }
}

fn attach_token_info(lex: &mut logos::Lexer<LexToken>)
        -> TokenInfo {
    TokenInfo{ loc: lex.span(), s: lex.slice().to_string()}
}

#[derive(Logos, Debug, Clone, PartialEq)]
pub enum LexToken {
    #[token("section", attach_token_info)]
    Section(TokenInfo),

    #[token("{", attach_token_info)]
    OpenBrace(TokenInfo),

    #[token("}", attach_token_info)]
    CloseBrace(TokenInfo),

    #[token(";", attach_token_info)]
    Semicolon(TokenInfo),

    #[regex("[_a-zA-Z][0-9a-zA-Z_]*", attach_token_info)]
    Identifier(TokenInfo),

    #[regex("0x[0-9a-fA-F]+|[1-9][0-9]*|0", attach_token_info)]
    Int(TokenInfo),

    // Not only is \ special in strings and must be escaped, but also special in
    // regex.  We use raw string here to avoid having the escape the \ for the
    // string itself. The \\ in this raw string are escape \ for the regex
    // engine underneath.
    #[regex(r#""(\\"|\\.|[^"])*""#, attach_token_info)]
    QuotedString(TokenInfo),

    #[regex(r#"/\*([^*]|\*[^/])+\*/"#, logos::skip)] // block comments
    #[regex(r#"//[^\r\n]*(\r\n|\n)?"#, logos::skip)] // line comments
    #[regex(r#"[ \t\n\f]+"#, logos::skip)]           // whitespace
    #[error]
    Unknown,
}


pub struct ASTNode<'toks> {
    tok : &'toks LexToken,
    childvec : Vec<ASTNode<'toks>>,
}

impl<'toks> ASTNode<'toks> {
    pub fn new(t :&'toks LexToken) -> ASTNode {
        ASTNode { childvec: vec![], tok : t }
    }

    pub fn add_child(&mut self, child: ASTNode<'toks>) {
        self.childvec.push(child)
    }
}

pub fn parse<'toks>(ltv : &'toks[LexToken], tn : &mut usize, parent : &'toks mut ASTNode<'toks>) {
    let t = &ltv[*tn];
    println!("Parsing token {}: {:?}", *tn, t);
    match t {
        LexToken::Section(_) => parse_section(ltv, tn, parent),
        _ => println!("Something else"),
    }
}

pub fn parse_section<'toks>(ltv : &'toks[LexToken], tn : &mut usize,
        parent : &'toks mut ASTNode<'toks>) {

    // get the section keyword
    let mut node = ASTNode::new(&ltv[*tn]);
    parent.add_child(node);
    *tn += 1;

    // After a section, an identifier is expected
    let t = &ltv[*tn];
    match t {
        LexToken::Identifier(_) => parse_identifier(ltv, tn, &mut node),
        _ => println!("Something else"),
    }
}

pub fn parse_identifier<'toks>(ltv : &'toks[LexToken], tn : &mut usize,
        parent : &'toks mut ASTNode<'toks>) {

    // get the section keyword
    let mut node = ASTNode::new(&ltv[*tn]);
    parent.add_child(node);
    *tn += 1;

    // After a section, an identifier is expected
    let t = &ltv[*tn];
    match t {
        LexToken::Identifier(_) => parse_identifier(ltv, tn, &mut node),
        _ => println!("Something else"),
    }
}

fn main() {
    let mut tv = Vec::new();

    let temp = "section foo{/*stu\nff*/92};// line \"quote\" comment\n section bar {0x56};\nsection foo {\"w\\\"o\nw\"}\n\nsection baz {0}";
    print!("Lexing:\n\n{}\n\n", temp);
    let lex = LexToken::lexer(temp);
    for t in lex {
        tv.push(t);
    }

    for t in tv {
        println!("LexToken = {:?}", t );
    }
}
