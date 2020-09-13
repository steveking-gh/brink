// Copyright (c) 2020 Steve King
// See license.txt.

#![warn(clippy::all)]

use logos::{Logos, Lexer};

#[derive(Logos, Debug, PartialEq)]
#[logos(extras = usize)]
enum Token {
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

    #[regex("[0-9]+")]
    DecimalInt,

    #[regex("0x[0-9a-fA-F]+")]
    HexInt,

    #[regex(r#""([^\\"]|\\.)*""#)]
    QuotedString,

    #[regex(r#"/\*([^*]|\*[^/])+\*/"#, logos::skip)] // block comments
    #[regex(r#"//[^\r\n]*(\r\n|\n)?"#, logos::skip)] // line comments
    #[regex(r#"[ \t\n\f]+"#, logos::skip)]           // whitespace
    #[error]
    Unknown,
}

fn main() {
    let temp = "section foo{/*stu\nff*/92};// line \"quote\" comment\n section bar {0x56};\nsection foo {\"w\\\"o\nw\"}";
    let lex = Token::lexer(temp);
    for t in lex {
        println!("Token = {:?}", t);
    }

}
