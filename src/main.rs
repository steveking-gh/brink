// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use std::vec::Vec;
use logos::{Logos};
use indextree::{Arena,NodeId};

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
    _Root_,

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


pub fn parse<'toks>(ltv: &'toks[LexToken], tn: &mut usize) -> Arena<usize> {
    let mut arena = Arena::new();

    // For uniformity, we want a root node as a parent for recursion
    // The LexToken index of the root should not get confused with an
    // actual index into ltv, so use MAX.
    let mut root = arena.new_node(usize::MAX);
    let t = &ltv[*tn];
    println!("Parsing token {}: {:?}", *tn, t);
    match t {
        LexToken::Section(_) => parse_section(&mut arena, ltv, tn, &mut root),
        _ => println!("Error"),
    }

    arena
}

pub fn parse_section<'toks>(arena : &mut Arena<usize>, ltv : &'toks[LexToken],
                            tn : &mut usize, parent : &mut NodeId) {

    // Add the section keyword as a child of the parent
    // All content in the section are children of the section node
    let mut node = arena.new_node(*tn);
    parent.append(node, arena);

    // Advance the token number past 'section'
    *tn += 1;

    // After a section declaration, an identifier is expected
    let t = &ltv[*tn];
    match t {
        LexToken::Identifier(_) => parse_identifier(arena, tn, &mut node),
        _ => println!("Error in section declaration"),
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

fn main() {
    let mut tv = Vec::new();

    let temp = "section foo{/*stu\nff*/92};// line \"quote\" comment\n section bar {0x56};\nsection foo {\"w\\\"o\nw\"}\n\nsection baz {0}";
    print!("Lexing:\n\n{}\n\n", temp);
    let lex = LexToken::lexer(temp);
    for t in lex {
        tv.push(t);
    }

    let mut tok_index = 0;
    let arena = parse(&tv, &mut tok_index);
    for node in arena.iter() {
        if *node.get() == usize::MAX {
            continue; // skip the fake root node
        }
        println!("Node = {} for token {:?}", *node.get(), tv[*node.get()] );
    }
}
