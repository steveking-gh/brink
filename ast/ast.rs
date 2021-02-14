use logos::{Logos};
use indextree::{Arena,NodeId};
pub type Span = std::ops::Range<usize>;
use std::{collections::{HashMap,HashSet}};
use std::option;
use diags::Diags;
use anyhow::{Context, bail};
use std::fs::File;
use std::io::prelude::*;


#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

/// All tokens in brink created with the logos macro.
/// Keep this simple and do not be tempted to attach
/// unstructured values these enum.
#[derive(Logos, Debug, Clone, Copy, PartialEq)]
pub enum LexToken {
    #[token("section")] Section,
    #[token("assert")] Assert,
    #[token("sizeof")] Sizeof,
    #[token("wrs")] Wrs,
    #[token("wr")] Wr,
    #[token("output")] Output,
    #[token("==")] EqEq,
    #[token("!=")] NEq,
    #[token("&")] Ampersand,
    #[token("|")] Pipe,
    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Asterisk,
    #[token("/")] FSlash,
    #[token("<<")] DoubleLess,
    #[token(">>")] DoubleGreater,
    #[token("{")] OpenBrace,
    #[token("}")] CloseBrace,
    #[token("(")] OpenParen,
    #[token(")")] CloseParen,
    #[token(";")] Semicolon,
    #[regex("[_a-zA-Z][0-9a-zA-Z_]*")] Identifier,

    // Support the forms accepted by the nice parse_int crate:
    // 0b, 0o, 0x and regular decimal
    #[regex("0[bB][01][_01]*|0[xX][0-9a-fA-F][_0-9a-fA-F]*|[1-9][_0-9]*|0")] U64,

    // Not only is \ special in strings and must be escaped, but also special in
    // regex.  We use raw string here to avoid having the escape the \ for the
    // string itself. The \\ in this raw string are escape \ for the regex
    // engine underneath.
    #[regex(r#""(\\"|\\.|[^"])*""#)] QuotedString,

    // Comments and whitespace are stripped from user input during processing.
    // This stripping happens *after* we record all the line/offset info
    // with codespan for error reporting.
    #[regex(r#"/\*([^*]|\*[^/])+\*/"#, logos::skip)] // block comments
    #[regex(r#"//[^\r\n]*(\r\n|\n)?"#, logos::skip)] // line comments
    #[regex(r#"[ \t\n\f]+"#, logos::skip)]           // whitespace
    #[error]
    Unknown,
}

/// The basic token info structure used everywhere.
/// The AST constructs a vector of TokenInfos.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo<'toks> {
    /// The token enum as identified by logos
    pub tok: LexToken,

    /// The range of bytes in the source file occupied
    /// by this token.  Diagnostics require this range
    /// when producing errors.
    pub loc: Span,

    /// The value of the token trimmed of whitespace
    pub val: &'toks str,
}

impl<'toks> TokenInfo<'toks> {
    pub fn span(&self) -> Span { self.loc.clone() }
}

/**
 * Abstract Syntax Tree
 * This structure contains the AST created from the raw lexical
 * tokens.  The lifetime of this struct is the same as the tokens.
 */
pub struct Ast<'toks> {
    /// The arena from the indextree crate holding all nodes
    /// in the AST.  Arenas are one idiomatic rust way to nicely
    /// manage the pointer craziness of trees
    arena: Arena<usize>,

    /// A vector of info about for tokens identified by logos.
    tv: Vec<TokenInfo<'toks>>,
 
    /// The artificial root of the tree.  The children of this
    /// tree are the top level tokens in the user's source file.
    root: NodeId,

    /// The current token number pointer within the tv
    tok_num: usize,
}

impl<'toks> Ast<'toks> {

    /// Peek at the next token info object, if any.
    fn peek(&self) -> Option<&TokenInfo<'toks>> {
        self.tv.get(self.tok_num)
    }

    /// Take the next token info object, if any.
    fn take(&mut self) -> Option<&TokenInfo<'toks>> {
        let tinfo = self.tv.get(self.tok_num);
        self.tok_num += 1;
        tinfo
    }

    /// Create a new abstract syntax tree.
    pub fn new(fstr: &'toks str, diags: &mut Diags) -> Option<Self> {
        let mut arena = Arena::new();
        let root = arena.new_node(usize::MAX);
        let mut tv = Vec::new();
        let mut lex = LexToken::lexer(fstr);
        while let Some(tok) = lex.next() {
            debug!("ast::new: Token {} = {:?}", tv.len(), tok);
            tv.push(TokenInfo{tok, val:lex.slice(), loc: lex.span()});
        }
        let mut ast = Self { arena, tv, root, tok_num: 0 };
        if !ast.parse(diags) {
            // ast construction failed.  Let the caller report
            // this in whatever way they want.
            return None;
        }

        Some(ast)
    }

    // Boilerplate entry for recursive descent parsing functions.
    fn dbg_enter(&self, func_name: &str) {
        if let Some(tinfo) = self.peek() {
            debug!("Ast::{} ENTER, {}:{} is {:?}", func_name, self.tok_num,
                   tinfo.val, tinfo.tok);
        } else {
            debug!("Ast::{} ENTER, {}:{} is {}", func_name, self.tok_num,
                   "<end of input>", "<end of input>");
        }
    }

    /// Boilerplate exit for recursive descent parsing functions.
    /// This function returns the result and should be the last statement
    /// in each function
    fn dbg_exit(&self, func_name: &str, result: bool) -> bool {
        debug!("Ast::{} EXIT {:?}", func_name, result);
        result
    }

    /// Boilerplate exit for recursive descent parsing functions.
    /// This function returns the result and should be the last statement
    /// in each function
    fn dbg_exit_pratt(&self, func_name: &str, result: Option<NodeId>) -> Option<NodeId> {
        debug!("Ast::{} EXIT {:?}", func_name, result);
        result
    }

    /// Return an iterator over the children of the specified AST node
    pub fn children(&'toks self, nid: NodeId) -> indextree::Children<usize>{
        return nid.children(&self.arena)
    }

    /// Returns the lexical value of the specified child of the specified
    /// parent. The value is always a string reference to source code regardless
    /// of the semantic meaning of the child.
    pub fn get_child_str(&'toks self, parent_nid: NodeId, child_num: usize) -> &'toks str {
        debug!("Ast::get_child_str: child number {} for parent nid {}", child_num, parent_nid);
        let mut children = parent_nid.children(&self.arena);
        let name_nid = children.nth(child_num).unwrap();
        let tinfo = self.get_tinfo(name_nid);
        tinfo.val
    }

    /// Parse the flat token vector to build the syntax tree. Unlike the flat
    /// vector of tokens, the tree represents the semantic parent-child relation
    /// between elements in the source file.  We check syntax and grammar during
    /// tree construction.
    fn parse(&mut self, diags: &mut Diags) -> bool {
        self.dbg_enter("parse");
        let toks_end = self.tv.len();
        debug!("Ast::parse: Total of {} tokens", toks_end);

        let mut result = true;
        while let Some(tinfo) = self.peek() {
            debug!("Ast::parse: Parsing token {}: {:?}", self.tok_num, tinfo);
            result &= match tinfo.tok {
                LexToken::Section => self.parse_section(self.root, diags),
                LexToken::Output => self.parse_output(self.root, diags),

                // Unrecognized top level token.  Report the error, but keep going
                // to try to give the user more errors in batches.
                _ => {
                    let msg = format!("Unrecognized token '{}' at top level scope", tinfo.val);
                    diags.err1("AST_18", &msg, tinfo.span());

                    // Skip the bad token.
                    self.tok_num += 1;
                    false
                },
            };
        }
        self.dbg_exit("parse", result)
    }

    fn err_expected_after(&self, diags: &mut Diags, code: &str, msg: &str) {
        let m = format!("{}, but found '{}'", msg, self.tv[self.tok_num].val);
        diags.err2(code, &m, self.tv[self.tok_num].span(), 
   self.tv[self.tok_num-1].span());
    }

    fn err_invalid_expression(&self, diags: &mut Diags, code: &str) {
        let m = format!("Invalid expression '{}'", self.tv[self.tok_num].val);
        diags.err1(code, &m, self.tv[self.tok_num].span());
    }

    fn err_no_input(&self, diags: &mut Diags) {
        diags.err0("AST_13", "Unexpected end of input");
    }

    fn err_no_close_brace(&self, diags: &mut Diags, brace_tok_num: usize) {
        let m = format!("Missing '}}'.  The following open brace is unmatched.");
        diags.err1("AST_14", &m, self.tv[brace_tok_num].span());
    }

    /// Attempts to advance the token number past the next semicolon.
    /// The final token number may be invalid.  This function is
    /// used to try to recover from syntax errors.
    fn advance_past_semicolon(&mut self) {
        self.dbg_enter("advance_past_semicolon");
        assert!(self.tok_num != 0);
        while let Some(tinfo) = self.take() {
            if tinfo.tok == LexToken::Semicolon {
                break;
            }
        }
        debug!("Ast::advance_past_semicolon: Stopped on token {}", self.tok_num);
        self.dbg_exit("advance_past_semicolon", true);
    }

    /// Add the specified token as a child of the parent.
    /// Advance the token number and return the new node ID for the input token.
    fn add_to_parent_and_advance(&mut self, parent: NodeId) -> NodeId {
        let nid = self.arena.new_node(self.tok_num);
        parent.append(nid, &mut self.arena);
        self.tok_num += 1;
        nid
    }

    fn expect_leaf(&mut self, diags: &mut Diags, parent : NodeId, expected_token: LexToken, code: &str,
        context: &str) -> bool {

        self.dbg_enter("expect_leaf");

        let mut result = false;

        if let Some(tinfo) = self.peek() {
            if expected_token == tinfo.tok {
                self.add_to_parent_and_advance(parent);
                result = true;
            } else {
                self.err_expected_after(diags, code, context);
            }
        } else {
            self.err_no_input(diags);
        }

        self.dbg_exit("expect_leaf", result)
    }

    /// Process an expected semicolon.  This function is just a convenient
    /// specialization of expect_leaf().
    fn expect_semi(&mut self, diags: &mut Diags, parent : NodeId) -> bool {

        if let Some(tinfo) = self.peek() {
            if LexToken::Semicolon == tinfo.tok {
                self.add_to_parent_and_advance(parent);
                return true;
            } else {
                self.err_expected_after(diags, "AST_17", "Expected ';'");
            }
        } else {
            self.err_no_input(diags);
        }

        false
    }

    /// Expect the specified token, add it to the parent and advance.
    fn expect_token(&mut self, tok: LexToken, diags: &mut Diags, parent : NodeId) -> bool {

        if let Some(tinfo) = self.peek() {
            if tok == tinfo.tok {
                self.add_to_parent_and_advance(parent);
                return true;
            } else {
                let msg = format!("Expected {:?}", tok);
                self.err_expected_after(diags, "AST_22", &msg);
            }
        } else {
            self.err_no_input(diags);
        }

        false
    }

    /// Expect the specified token and advance without adding to the parent.
    fn expect_token_no_add(&mut self, tok: LexToken, diags: &mut Diags) -> bool {

        if let Some(tinfo) = self.peek() {
            if tok == tinfo.tok {
                self.tok_num += 1;
                return true;
            } else {
                let msg = format!("Expected {:?}", tok);
                self.err_expected_after(diags, "AST_22", &msg);
            }
        } else {
            self.err_no_input(diags);
        }

        false
    }    

    /// Parse a section definition.
    fn parse_section(&mut self, parent : NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_section");
        let mut result = false;
        // Sections are always children of the root node, but no need to make
        // that a special case here.
        let sec_nid = self.add_to_parent_and_advance(parent);

        // After 'section' an identifier is expected
        if self.expect_leaf(diags, sec_nid, LexToken::Identifier, "AST_1",
                     "Expected an identifier after section") {
            // After a section identifier, expect an open brace.
            // Remember the location of the opening brace to help with
            // user missing brace errors.
            let brace_toknum = self.tok_num;
            if self.expect_leaf(diags, sec_nid, LexToken::OpenBrace, "AST_2",
                         "Expected { after identifier") {
                result = self.parse_section_contents(sec_nid, diags, brace_toknum);
            }
        }
        self.dbg_exit("parse_section", result)
    }

    /// Parse all possible content within a section.
    fn parse_section_contents(&mut self, parent : NodeId, diags: &mut Diags,
                              brace_tok_num: usize) -> bool {

        self.dbg_enter("parse_section_contents");
        let mut result = true; // todo fixme

        while let Some(tinfo) = self.peek() {
            debug!("Ast::parse_section_contents: token {}:{}", self.tok_num, tinfo.val);
            // todo rewrite as match statement
            // When we find a close brace, we're done with section content
            if tinfo.tok == LexToken::CloseBrace {
                self.parse_leaf(parent);
                return self.dbg_exit("parse_section_contents", result);
            }

            // Stay in the section even after errors to give the user
            // more than one error at a time
            let parse_ok = match tinfo.tok {
                LexToken::Wr => self.parse_wr(parent, diags),
                LexToken::Wrs => self.parse_wrs(parent, diags),
                LexToken::Assert => self.parse_assert(parent, diags),
                _ => {
                    self.err_invalid_expression(diags, "AST_3");
                    false
                }
            };

            if !parse_ok {
                self.take();
                debug!("Ast::parse_section_contents: skipping to next ; starting from {}", self.tok_num);
                // Consume the bad token and skip forward    
                self.advance_past_semicolon();
                result = false;
            }
        }

        // If we got here, we ran out of tokens before finding the close brace.
        self.err_no_close_brace(diags, brace_tok_num);
        return self.dbg_exit("parse_section_contents", false);
    }

    // Parser for writing a section
    fn parse_wr(&mut self, parent_nid : NodeId, diags: &mut Diags) -> bool {

        self.dbg_enter("parse_wr");
        let mut result = false;

        // Add the wr keyword as a child of the parent and advance
        let wr_nid = self.add_to_parent_and_advance(parent_nid);

        // Next, an identifier (section name) is expected
        if self.expect_leaf(diags, wr_nid, LexToken::Identifier, "AST_15",
                    "Expected a section name after 'wr'") {
            result = self.expect_semi(diags, wr_nid);
        }
        self.dbg_exit("parse_wr", result)
    }

    /// Parser for writing a string
    fn parse_wrs(&mut self, parent_nid : NodeId, diags: &mut Diags) -> bool {

        self.dbg_enter("parse_wrs");
        let mut result = false;
        // Add the wrs keyword as a child of the parent and advance
        let wrs_nid = self.add_to_parent_and_advance(parent_nid);

        // Next, a quoted string is expected
        if self.expect_leaf(diags, wrs_nid, LexToken::QuotedString, "AST_4",
                    "Expected a quoted string after 'wrs'") {
            result = self.expect_semi(diags, wrs_nid);
        }
        self.dbg_exit("parse_wrs", result)
    }

    /// Returns the (lhs,rhs) binding power for any token
    /// Higher numbers are stronger binding.
    fn get_binding_power(tok: LexToken) -> (u8,u8) {
        match tok {
            LexToken::U64 => (11,12),
            LexToken::FSlash |
            LexToken::Asterisk => (9,10),
            LexToken::Minus |
            LexToken::Plus => (7,8),
            LexToken::Ampersand |
            LexToken::Pipe => (5,6),
            LexToken::DoubleGreater |
            LexToken::DoubleLess => (3,4),
            LexToken::NEq |
            LexToken::EqEq => (1,2),
            _ => (0,0),
        }
    }

    /// Parse an expression with correct precedence up to the next semicolon.
    /// This is a Pratt parser that returns the NodeID at the top of the local AST.
    /// See https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html
    /// for a nice explanation of Pratt parsers with Rust.
    /// On return, the terminal semicolon will be the next unprocessed token.
    fn parse_pratt(&mut self, min_bp: u8, result: &mut bool, diags: &mut Diags) -> Option<NodeId> {

        self.dbg_enter("parse_pratt");
        debug!("Ast::parse_pratt: Min BP = {}", min_bp);
        let lhs_tinfo = self.peek();
        if lhs_tinfo.is_none() {
            *result = false;
            self.err_no_input(diags);
            return self.dbg_exit_pratt("parse_pratt", None);
        }

        let lhs_tinfo = lhs_tinfo.unwrap();

        let mut lhs_nid = match lhs_tinfo.tok {
            LexToken::CloseParen |          // Finding a closing paren means we're done
            LexToken::Semicolon => { None } // Finding a semicolon means we're done.
            LexToken::OpenParen => {
                self.tok_num += 1;
                let mut lhs = self.parse_pratt(0, result, diags);
                if lhs.is_some() {
                    if let Some(paren_tinfo) = self.peek() {
                        if paren_tinfo.tok == LexToken::CloseParen {
                            self.tok_num += 1;
                        } else {
                            let msg = format!("Expected a closing ')' but found {}", paren_tinfo.val);
                            diags.err1("AST_20", &msg, paren_tinfo.span());
                            *result = false;
                            lhs = None;
                        }
                    }
                }
                lhs
            }
            LexToken::U64 => {
                let lhs = Some(self.arena.new_node(self.tok_num));
                self.tok_num += 1;
                lhs
            }
            LexToken::Sizeof => {
                // We expect 3 tokens: '(' 'identifier' ')', but don't bother to record
                // the surrounding parens to simplify later linearization
                let assert_nid = self.arena.new_node(self.tok_num);
                self.tok_num += 1;
                if !self.expect_token_no_add(LexToken::OpenParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", None);
                }
                if !self.expect_token(LexToken::Identifier, diags, assert_nid) {
                    return self.dbg_exit_pratt("parse_pratt", None);
                }
                if !self.expect_token_no_add(LexToken::CloseParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", None);
                }

                let lhs = Some(assert_nid);
                lhs
            }
            _ => {
                let msg = format!("Invalid expression operand '{}'", lhs_tinfo.val);
                diags.err1("AST_19", &msg, lhs_tinfo.span());
                None
            }
        };

        if lhs_nid.is_none() {
            return self.dbg_exit_pratt("parse_pratt", None);
        }

        loop {

            // We expect an operation such as add, a semicolon, or EOF.
            let op_tinfo = self.peek();
            if op_tinfo.is_none() {
                break;
            }

            let op_tinfo = op_tinfo.unwrap();
            match op_tinfo.tok {
                LexToken::Semicolon => { break; }
                LexToken::NEq |
                LexToken::EqEq |
                LexToken::DoubleGreater |
                LexToken::DoubleLess |
                LexToken::Ampersand |
                LexToken::Pipe |
                LexToken::Plus |
                LexToken::Minus |
                LexToken::Asterisk |
                LexToken::FSlash => {}
                _ => {
                    let msg = format!("Invalid operation '{}'", op_tinfo.val);
                    diags.err1("AST_18", &msg, op_tinfo.span());
                    break;
                }
            }

            let (lbp,rbp) = Ast::get_binding_power(op_tinfo.tok);

            debug!("Ast::parse_pratt: operation '{}' with (lbp,rbp) = ({},{})",
                    op_tinfo.val, lbp, rbp );

            // A decrease in operator precedence ends the iteration.
            if lbp < min_bp {
                break;
            }

            let op_nid = self.arena.new_node(self.tok_num);
            self.tok_num += 1;

            // attach the left hand size as a child of the operation
            op_nid.append(lhs_nid.unwrap(), &mut self.arena);

            // The operation is the new left-hand-side from our caller's point of view
            lhs_nid = Some(op_nid);

            // Recurse into the right hand side of the operation, if any
            let rhs_nid = self.parse_pratt(rbp, result, diags);

            if let Some(rhs_nid) = rhs_nid {
                op_nid.append(rhs_nid, &mut self.arena);
            } else {
                // RHS is none
                break;
            }
        }

    
        self.dbg_exit_pratt("parse_pratt", lhs_nid)
    }

    /// Parser for an assert statement
    /// We do not yet have full mathematical expression evaluation.
    /// The assert must be a 3 part expression with the middle lexical element
    /// either a == or !=.
    fn parse_assert(&mut self, parent: NodeId, diags: &mut Diags) -> bool {

        self.dbg_enter("parse_assert");
        let mut result = true;
        // Add the assert keyword as a child of the parent
        let assert_nid = self.add_to_parent_and_advance(parent);
        let expression_nid = self.parse_pratt(0, &mut result, diags);
        if result {
            if let Some(expression_nid) = expression_nid {
                assert_nid.append(expression_nid, &mut self.arena);
                result &= self.expect_semi(diags, assert_nid);
            }
        }

        self.dbg_exit("parse_assert", result)
    }

    fn parse_output(&mut self, parent : NodeId, diags: &mut Diags) -> bool {

        self.dbg_enter("parse_output");
        let mut result = false;
        // Add the section keyword as a child of the parent and advance
        let output_nid = self.add_to_parent_and_advance(parent);

        // After 'output' a section identifier is expected
        if self.expect_leaf(diags, output_nid, LexToken::Identifier, "AST_7",
                    "Expected a section name after output") {
            result = self.expect_semi(diags, output_nid);
        }

        self.dbg_exit("parse_output", result)
    }

    
     /// Adds the current token as a child of the parent and advances
     /// the token index.  The current token MUST BE VALID!
    fn parse_leaf(&mut self, parent : NodeId) {
        let nid = self.arena.new_node(self.tok_num);
        parent.append(nid, &mut self.arena);
        self.tok_num += 1;
    }

    pub fn get_tinfo(&self, nid: NodeId) -> &'toks TokenInfo {
        let tok_num = *self.arena[nid].get();
        &self.tv[tok_num]
    }

    const DOT_DEFAULT_FILL: &'static str = "#F2F2F2";
    const DOT_DEFAULT_EDGE: &'static str = "#808080";
    const DOT_DEFAULT_PEN: &'static str = "#808080";

    fn dump_r(&self, nid: NodeId, depth: usize, file: &mut File) ->anyhow::Result<()> {
        debug!("AST: {}: {}{}", nid, " ".repeat(depth * 4), self.get_tinfo(nid).val);
        let tinfo = self.get_tinfo(nid);

        let (label,color) = match tinfo.tok {
            LexToken::Section |
            LexToken::Wr |
            LexToken::Wrs |
            LexToken::Output => (tinfo.val, Ast::DOT_DEFAULT_FILL),
            LexToken::Identifier => (tinfo.val, Ast::DOT_DEFAULT_FILL),
            LexToken::QuotedString => {
                if tinfo.val.len() <= 8 {
                    (tinfo.val.trim_matches('\"'), Ast::DOT_DEFAULT_FILL)
                } else {
                    ("<string>", Ast::DOT_DEFAULT_FILL)
                }
            }
            LexToken::Unknown => ("<unknown>", "red"),
            _ => (tinfo.val,Ast::DOT_DEFAULT_FILL)
        };

        file.write(format!("{} [label=\"{}\",fillcolor=\"{}\"]\n",nid,label,color)
                .as_bytes()).context("ast.dot write failed")?;
        let children = nid.children(&self.arena);
        for child_nid in children {

            /*
            let child_tinfo = self.get_tinfo(child_nid);
            if child_tinfo.tok == LexToken::Semicolon {
                continue;
            }
            */

            file.write(format!("{} -> {}\n", nid, child_nid).as_bytes()).context("ast.dot write failed")?;
            self.dump_r(child_nid, depth+1, file)?;
        }
        Ok(())
    }

    /**
     * Recursively dumps the AST to the console.
     */
    pub fn dump(&self, fname : &str) -> anyhow::Result<()> {

        debug!("");

        let mut file = File::create(fname).context(
            format!("Error attempting to create debug file '{}'", fname))?;
            file.write(b"digraph {\n").context("ast.dot write failed")?;
            file.write(format!("node [style=filled,fillcolor=\"{}\",color=\"{}\"]\n",
                    Ast::DOT_DEFAULT_FILL,Ast::DOT_DEFAULT_PEN).as_bytes()).context("ast.dot write failed")?;
            file.write(format!("edge [color=\"{}\"]\n",
                    Ast::DOT_DEFAULT_EDGE).as_bytes()).context("ast.dot write failed")?;

            file.write(format!("{} [label=\"root\"]\n",self.root).as_bytes()).context("ast.dot write failed")?;
            let children = self.root.children(&self.arena);
            for child_nid in children {
                file.write(format!("{} -> {}\n",self.root, child_nid).as_bytes()).context("ast.dot write failed")?;
                self.dump_r(child_nid, 0, &mut file)?;
            }

        file.write(b"}\n").context("ast.dot write failed")?;
        debug!("");
        Ok(())
    }
}

/*******************************
 * Section
 ******************************/
 #[derive(Debug)]
 pub struct Section<'toks> {
    pub tinfo: &'toks TokenInfo<'toks>,
    pub nid: NodeId,
}

impl<'toks> Section<'toks> {
    pub fn new(ast: &'toks Ast, nid: NodeId) -> Section<'toks> {
        Section { tinfo: ast.get_tinfo(nid), nid }
    }
}

/*******************************
 * Output
 ******************************/
#[derive(Clone, Debug)]
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
        let sec_tinfo = ast.get_tinfo(sec_nid);
        let sec_str = sec_tinfo.val;
        Output { tinfo: ast.get_tinfo(nid), nid, sec_nid, sec_str}
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
    pub output: Output<'toks>,
    //pub properties: HashMap<NodeId, NodeProperty>
}

impl<'toks> AstDb<'toks> {

    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH:usize = 100;

    /// Processes a section in the AST
    fn record_section(diags: &mut Diags, sec_nid: NodeId, ast: &'toks Ast,
                      sections: &mut HashMap<&'toks str, Section<'toks>> ) -> bool {
        debug!("AstDb::record_section: NodeId {}", sec_nid);

        let mut children = sec_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        if sections.contains_key(sec_str) {
            // error, duplicate section names
            // We know the section exists, so unwrap is fine.
            let orig_section = sections.get(sec_str).unwrap();
            let orig_tinfo = orig_section.tinfo;
            let m = format!("Duplicate section name '{}'", sec_str);
            diags.err2("AST_9", &m, sec_tinfo.span(), orig_tinfo.span());
            return false;
        }
        sections.insert(sec_str, Section::new(&ast, sec_nid));
        true
    }

    /// Returns true if the first child of the specified node is a section
    /// name that exists.  Otherwise, prints a diagnostic and returns false.
    fn validate_section_name(child_num: usize, parent_nid: NodeId, ast: &'toks Ast,
                    sections: &HashMap<&'toks str, Section<'toks>>, diags: &mut Diags) -> bool {
        debug!("AstDb::validate_section_name: NodeId {} for child {}", parent_nid, child_num);

        let mut children = parent_nid.children(&ast.arena);

        // First, advance to the specified child number
        let mut child_num = 0;
        while child_num < child_num {
            let sec_name_nid_opt = children.next();
            if sec_name_nid_opt.is_none() {
                // error, not enough children to reach section name
                let m = format!("Missing section name");
                let section_tinfo = ast.get_tinfo(parent_nid);
                diags.err1("AST_23", &m, section_tinfo.span());
                return false;
            }
            child_num += 1;
        }
        let sec_name_nid_opt = children.next();
        if sec_name_nid_opt.is_none() {
            // error, specified section does not exist
            let m = format!("Missing section name");
            let section_tinfo = ast.get_tinfo(parent_nid);
            diags.err1("AST_11", &m, section_tinfo.span());
            return false;
        }
        let sec_name_nid = sec_name_nid_opt.unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        if !sections.contains_key(sec_str) {
            // error, specified section does not exist
            let m = format!("Unknown section name '{}'", sec_str);
            diags.err1("AST_16", &m, sec_tinfo.span());
            return false;
        }
        true
    }

    pub fn record_output(diags: &mut Diags, nid: NodeId, ast: &'toks Ast,
                         output: &mut option::Option<Output<'toks>>) -> bool {
        let tinfo = ast.get_tinfo(nid);
        if output.is_some() {
            let m = "Multiple output statements are not allowed.";
            let orig_tinfo = output.as_ref().unwrap().tinfo;
            diags.err2("AST_10", &m, orig_tinfo.span(), tinfo.span());
            return false;
        }

        *output = Some(Output::new(&ast,nid));
        true // succeed
    }

    /// Recursively record information about the children of an AST object.
    /// nested sections tracks the current hierarchy of section writes so we
    /// catch cycles.
    fn record_r(rdepth: usize, parent_nid: NodeId, diags: &mut Diags,
        ast: &'toks Ast, sections: &HashMap<&'toks str, Section<'toks>>,
        nested_sections: &mut HashSet<&'toks str> ) -> bool {

        debug!("AstDb::record_r: >>>> ENTER at depth {} for parent nid: {}",
                rdepth, parent_nid);

        if rdepth > AstDb::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!("Maximum recursion depth ({}) exceeded when processing '{}'.",
                            AstDb::MAX_RECURSION_DEPTH, tinfo.val);
            diags.err1("AST_5", &m, tinfo.span());
            return false;
        }

        let mut result = true;
        let tinfo = ast.get_tinfo(parent_nid);
        result &= match tinfo.tok {
            // Wr statement must specify a valid section name
            LexToken::Wr => {
                if !Self::validate_section_name(0, parent_nid, &ast, &sections, diags) {
                    return false;
                }
                let mut children = parent_nid.children(&ast.arena);
                // the section name is the first child of the output
                // AST processing guarantees this exists.
                let sec_nid = children.next().unwrap();
                let sec_tinfo = ast.get_tinfo(sec_nid);
                let sec_str = sec_tinfo.val;

                // Make sure we haven't already recursed through this section.
                if nested_sections.contains(sec_str) {
                    let m = "Writing section creates a cycle.";
                    diags.err1("AST_6", &m, sec_tinfo.span());
                    false
                } else {
                    // add this section to our nested sections tracker
                    nested_sections.insert(sec_str);
                    let section = sections.get(sec_str).unwrap();
                    let children = section.nid.children(&ast.arena);
                    for nid in children {
                        result &= AstDb::record_r(rdepth + 1, nid, diags, ast, sections, nested_sections);
                    }
                    result
                }
            }
            // Sizeof statement must specify a valid section name enclosed in ()
            LexToken::Sizeof => {
                // child 0 is the identifier since we didn't record surround '()'
                if !Self::validate_section_name(0, parent_nid, &ast, &sections, diags) {
                    return false;
                }
                let children = parent_nid.children(&ast.arena);
                for nid in children {
                    result &= AstDb::record_r(rdepth + 1, nid, diags, ast, sections, nested_sections);
                }
                result
            }
            _ => {
                // When no children exist, this case terminates recursion.
                let children = parent_nid.children(&ast.arena);
                for nid in children {
                    result &= AstDb::record_r(rdepth + 1, nid, diags, ast, sections, nested_sections);
                }
                result
            }
        };

        debug!("AstDb::record_r: <<<< EXIT({}) at depth {} for nid: {}",
                result, rdepth, parent_nid);
        result
    }

    pub fn new(diags: &mut Diags, ast: &'toks Ast) -> anyhow::Result<AstDb<'toks>> {
        debug!("AstDb::new");

        // Populate the AST database of critical structures.
        let mut result = true;

        let mut sections: HashMap<&'toks str, Section<'toks>> = HashMap::new();
        let mut output: Option<Output<'toks>> = None;

        // First phase, record all sections and the output.
        // Sections are defined only at top level so no need for recursion.
        for nid in ast.root.children(&ast.arena) {
            let tinfo = ast.get_tinfo(nid);
            result = result && match tinfo.tok {
                LexToken::Section => Self::record_section(diags, nid, &ast, &mut sections),
                LexToken::Output => Self::record_output(diags, nid, &ast, &mut output),
                _ => true, // other statements are of no consequence here
            };
        }

        if !result {
            bail!("AST construction failed");
        }

        // Make sure we found an output!
        if output.is_none() {
            diags.err0("AST_8", "Missing output statement");
            bail!("AST construction failed");
        }

        //let mut nested_sections : HashSet<&'toks str> = HashSet::new();
        let mut nested_sections = HashSet::new();

        let output_nid = output.as_ref().unwrap().nid;

        if !Self::validate_section_name(0, output_nid, &ast, &sections, diags) {
            bail!("AST construction failed");
        }

        let mut children = output_nid.children(&ast.arena);
        // the section name is the first child of the output
        // AST processing guarantees this exists.
        let sec_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_nid);
        let sec_str = sec_tinfo.val;

        // add the output section to our nested sections tracker
        nested_sections.insert(sec_str);
        let section = sections.get(sec_str).unwrap();
        let children = section.nid.children(&ast.arena);
        for nid in children {
            result &= AstDb::record_r(1, nid, diags, ast, &sections, &mut nested_sections);
        }

        if !result {
            bail!("AST construction failed");
        }

        Ok(AstDb { sections, output: output.unwrap()})
    }
}
