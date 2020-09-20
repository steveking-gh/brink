// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use logos::{Logos};

pub type Span = std::ops::Range<usize>;

struct TokenInfo<'source> {
    loc : Span,
    s : &'source str,
}

impl<'source> TokenInfo<'source> {
    pub fn span(&self) -> Span { self.loc.clone() }
    pub fn slice(&self) -> &'source str { self.s }
}

#[derive(Logos, Debug, Clone, PartialEq)]
enum LexToken {
    #[token("section")]
    Section,

    #[token("{")]
    OpenBrace,

    #[token("}")]
    CloseBrace,

    #[token(";")]
    Semicolon,

    #[regex("[_a-zA-Z][0-9a-zA-Z_]*")]
    Identifier,

    #[regex("[1-9][0-9]*|0")]
    DecimalInt,

    #[regex("0x[0-9a-fA-F]+")]
    HexInt,

    #[regex(r#""([^\\"]|\\.)*""#)] // " fix syntax highlighting
    QuotedString,

    #[regex(r#"/\*([^*]|\*[^/])+\*/"#, logos::skip)] // block comments
    #[regex(r#"//[^\r\n]*(\r\n|\n)?"#, logos::skip)] // line comments
    #[regex(r#"[ \t\n\f]+"#, logos::skip)]           // whitespace
    #[error]
    Unknown,
}


fn main() {
    let temp = "section foo{/*stu\nff*/92};// line \"quote\" comment\n section bar {0x56};\nsection foo {\"w\\\"o\nw\"}";
    print!("Lexing:\n\n{}\n\n", temp);
    let lex = LexToken::lexer(temp);
    for t in lex {
        println!("LexToken = {:?}", t );
    }

}
