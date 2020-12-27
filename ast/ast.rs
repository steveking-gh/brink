use logos::{Logos};
use indextree::{Arena,NodeId};
pub type Span = std::ops::Range<usize>;
use std::collections::{HashMap,HashSet};
use std::option;
use anyhow::{bail};
use diags::Diags;
use anyhow::{Context};
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
    #[token("wrs")] Wrs,
    #[token("wr")] Wr,
    #[token("output")] Output,
    #[token("==")] EqEq,
    #[token("!=")] NEq,
    #[token("{")] OpenBrace,
    #[token("}")] CloseBrace,
//    #[token("(")] OpenParen,
//    #[token(")")] CloseParen,
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

/// The basic token info structure used everywhere.
/// The AST constructs a vector of TokenInfos.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo<'toks> {
    /// The token enum as identified by logos
    pub tok : LexToken,

    /// The range of bytes in the source file occupied
    /// by this token.  Diagnostics require this range
    /// when producing errors.
    pub loc : Span,

    /// The value of the token.
    pub val : &'toks str,
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
    pub arena: Arena<usize>,

    /// A vector of info about for tokens identified by logos.
    pub tv: Vec<TokenInfo<'toks>>,

    /// The artificial root of the tree.  The children of this
    /// tree are the top level tokens in the user's source file.
    pub root: NodeId,
}

impl<'toks> Ast<'toks> {

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
        let mut ast = Self { arena, tv, root };
        if !ast.parse(diags) {
            // ast construction failed.  Let the caller report
            // this in whatever way they want.
            return None;
        }

        Some(ast)
    }

    // Boilerplate entry for recursive descent parsing functions.
    fn dbg_enter(&self, func_name: &str, tok_num: usize) {
        if tok_num < self.tv.len() {
            debug!("Ast::{} >>>> ENTER, {}:{} is {:?}", func_name, tok_num,
                   self.tv[tok_num].val, self.tv[tok_num].tok);
        } else {
            debug!("Ast::{} >>>> ENTER, {}:{} is {}", func_name, tok_num,
                   "<tok out of range>", "<tok out of range>");
        }
    }

    // Boilerplate exit for recursive descent parsing functions.
    // This function returns the result and should be the last statement
    // in each function
    fn dbg_exit(&self, func_name: &str, result: bool) -> bool {
        debug!("Ast::{} <<<< EXIT {:?}", func_name, result);
        result
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
        self.dbg_enter("parse", 0);
        let toks_end = self.tv.len();
        debug!("Ast::parse: Total of {} tokens", toks_end);

        let mut tok_num = 0;

        // We can't simply iterate on the token vector because the loop consumes
        // tokens from the vector recursively in varying amounts.
        //
        // Complete the loop even if some parsing fails to give the user more
        // errors at a time.
        let mut success = true;
        while tok_num < toks_end {
            let tinfo = &self.tv[tok_num];
            debug!("Ast::parse: Parsing token {}: {:?}", &mut tok_num, tinfo);
            success &= match tinfo.tok {
                LexToken::Section => self.parse_section(&mut tok_num, self.root, diags),
                LexToken::Output => self.parse_output(&mut tok_num, self.root, diags),
                _ => {tok_num += 1; false },
            };
        }
        self.dbg_exit("parse", success)
    }

    fn err_expected_after(&self, diags: &mut Diags, code: &str, msg: &str, tok_num: &usize) {
        let m = format!("{}, but found '{}'", msg, self.tv[*tok_num].val);
        diags.err2(code, &m, self.tv[*tok_num].span(), self.tv[*tok_num-1].span());
    }

    fn err_invalid_expression(&self, diags: &mut Diags, code: &str, tok_num: &usize) {
        let m = format!("Invalid expression '{}'", self.tv[*tok_num].val);
        diags.err1(code, &m, self.tv[*tok_num].span());
    }

    fn err_no_input(&self, diags: &mut Diags) {
        diags.err0("AST_13", "Unexpected end of input");
    }

    fn err_no_close_brace(&self, diags: &mut Diags, brace_tok_num: usize) {
        let m = format!("Missing '}}'.  The following open brace is unmatched.");
        diags.err1("AST_14", &m, self.tv[brace_tok_num].span());
    }

    /// Attempts to advance the token number past the next semicolon
    /// The token number returned may be invalid.  This function is
    /// used to try to recover from syntax errors.
    fn advance_past_semicolon(&self, tok_num: usize) -> usize {
        let mut tnum = tok_num;
        while let Some(tinfo) = self.tv.get(tnum) {
            tnum += 1;
            if tinfo.tok == LexToken::Semicolon {
                break;
            }
        }
        tnum
    }

    /// Add the specified token as a child of the parent.
    /// Advance the token number and return the new node ID for the input token.
    fn add_to_parent_and_advance(&mut self, tok_num: &mut usize, parent: NodeId) -> NodeId {
        let nid = self.arena.new_node(*tok_num);
        parent.append(nid, &mut self.arena);
        *tok_num += 1;
        nid
    }

    fn expect_leaf(&mut self, diags: &mut Diags, tok_num : &mut usize,
        parent : NodeId, expected_token: LexToken, code: &str,
        context: &str) -> bool {

        self.dbg_enter("expect_leaf", *tok_num);

        let mut result = false;

        if let Some(tinfo) = self.tv.get(*tok_num) {
            if expected_token == tinfo.tok {
                self.add_to_parent_and_advance(tok_num, parent);
                result = true;
            } else {
                self.err_expected_after(diags, code, context, tok_num);
            }
        } else {
            self.err_no_input(diags);
        }

        self.dbg_exit("expect_leaf", result)
    }

    /// Process an expected semicolon.  This function is just a convenient
    /// specialization of expect_leaf().
    fn expect_semi(&mut self, diags: &mut Diags, tok_num : &mut usize,
                   parent : NodeId) -> bool {

        if let Some(tinfo) = self.tv.get(*tok_num) {
            if LexToken::Semicolon == tinfo.tok {
                self.add_to_parent_and_advance(tok_num, parent);
                return true;
            } else {
                self.err_expected_after(diags, "AST_17", "Expected ';'", tok_num);
            }
        } else {
            self.err_no_input(diags);
        }

        false
    }

    fn parse_section(&mut self, tok_num : &mut usize, parent : NodeId,
                    diags: &mut Diags) -> bool {


        self.dbg_enter("parse_section", *tok_num);
        let mut result = false;
        // Sections are always children of the root node, but no need to make
        // that a special case here.
        let sec_nid = self.add_to_parent_and_advance(tok_num, parent);

        // After 'section' an identifier is expected
        if self.expect_leaf(diags, tok_num, sec_nid, LexToken::Identifier, "AST_1",
                            "Expected an identifier after section") {
            // After a section identifier, expect an open brace.
            // Remember the location of the opening brace to help with
            // user missing brace errors.
            let brace_toknum = *tok_num;
            if self.expect_leaf(diags, tok_num, sec_nid, LexToken::OpenBrace, "AST_2",
                                "Expected { after identifier") {
                result = self.parse_section_contents(tok_num, sec_nid, diags, brace_toknum);
            }
        }
        self.dbg_exit("parse_section", result)
    }

    /// Parse all possible content within a section.
    fn parse_section_contents(&mut self, tok_num : &mut usize, parent : NodeId,
                              diags: &mut Diags, brace_tok_num: usize) -> bool {

        self.dbg_enter("parse_section_contents", *tok_num);
        let mut success = true; // todo fixme
        while let Some(tinfo) = self.tv.get(*tok_num) {
            debug!("Ast::parse_section_contents: token {}:{}", *tok_num, tinfo.val);
            // todo rewrite as match statement
            // When we find a close brace, we're done with section content
            if tinfo.tok == LexToken::CloseBrace {
                self.parse_leaf(tok_num, parent);
                return self.dbg_exit("parse_section_contents", success);
            }

            // Stay in the section even after errors to give the user
            // more than one error at a time
            let parse_ok = match tinfo.tok {
                LexToken::Wr => self.parse_wr(tok_num, parent, diags),
                LexToken::Wrs => self.parse_wrs(tok_num, parent, diags),
                LexToken::Assert => self.parse_assert(tok_num, parent, diags),
                _ => {
                    self.err_invalid_expression(diags, "AST_3", tok_num);
                    *tok_num += 1;
                    false
                }
            };

            // If something went wrong, then advance to the next semi
            // and try to keep going to give users more errors to fix.
            if !parse_ok {
                debug!("Ast::parse_section_contents: skipping to next ; starting from {}", *tok_num);
                *tok_num = self.advance_past_semicolon(*tok_num);
                success = false;
            }
        }

        // If we got here, we ran out of tokens before finding the close brace.
        self.err_no_close_brace(diags, brace_tok_num);
        return self.dbg_exit("parse_section_contents", false);
    }

    // Parser for writing a section
    fn parse_wr(&mut self, tok_num : &mut usize, parent_nid : NodeId,
                diags: &mut Diags) -> bool {

        self.dbg_enter("parse_wr", *tok_num);
        let mut result = false;

        // Add the wr keyword as a child of the parent and advance
        let wr_nid = self.add_to_parent_and_advance(tok_num, parent_nid);

        // Next, an identifier (section name) is expected
        if self.expect_leaf(diags, tok_num, wr_nid, LexToken::Identifier, "AST_15",
                             "Expected a section name after 'wr'") {
            result = self.expect_semi(diags, tok_num, wr_nid);
        }
        self.dbg_exit("parse_wr", result)
    }

    /// Parser for writing a string
    fn parse_wrs(&mut self, tok_num : &mut usize, parent_nid : NodeId,
                diags: &mut Diags) -> bool {

        self.dbg_enter("parse_wrs", *tok_num);
        let mut result = false;
        // Add the wrs keyword as a child of the parent and advance
        let wrs_nid = self.add_to_parent_and_advance(tok_num, parent_nid);

        // Next, a quoted string is expected
        if self.expect_leaf(diags, tok_num, wrs_nid, LexToken::QuotedString, "AST_4",
                             "Expected a quoted string after 'wrs'") {
            result = self.expect_semi(diags, tok_num, wrs_nid);
        }
        self.dbg_exit("parse_wrs", result)
    }

    /// Returns the (lhs,rhs) binding power for any token
    /// Higher numbers are stronger binding.
    fn get_binding_power(tok: LexToken) -> (u8,u8) {
        match tok {
            LexToken::NEq |
            LexToken::EqEq => (1,2),
            _ => (9,10),
        }
    }

    /// Parse an expression with correct precedence up to the next semicolon.
    fn parse_expr(&mut self, tok_num: &mut usize, prev_nid: NodeId,
                  diags: &mut Diags, prev_rbp: u8) -> bool {

        self.dbg_enter("parse_expr", *tok_num);
        let mut result = false;

        if let Some(tinfo) = self.tv.get(*tok_num) {
            // If we've finally found a semicolon, stop recursing.
            // The caller will deal with where to attach the semicolon.
            if tinfo.tok == LexToken::Semicolon {
                result = true;
            } else {
                let nid = self.arena.new_node(*tok_num);
                let (lbp, rbp) = Ast::get_binding_power(tinfo.tok);
                debug!("ast::parse_expr: tok = {} with ({},{})", tinfo.val, lbp, rbp);
                if lbp < prev_rbp {
                    // The left side binding power (lbp) of the current token
                    // is lower than the previous token's right side binding power (rbp).
                    // Therefore, we must evaluate the previous token first.
                    // The previous token becomes a child of the current token.
                    // We detach the previous token from its former parent and
                    // attach the current token in its place.
                    debug!("{} is parent of {}", nid, prev_nid);
                    // we expect only a single parent!
                    assert!(prev_nid.ancestors(&mut self.arena).count() >= 2);
                    // The first ancestor is the node itself (strange!)
                    // The next ancestor is the actual parent node we're looking for.
                    // Therefore, use a skip(1) to skip past this node.
                    // additional ancestors exist all the way back to the root.
                    let old_parent = prev_nid.ancestors(&mut self.arena).skip(1).next().unwrap();
                    prev_nid.detach(&mut self.arena);
                    old_parent.append(nid, &mut self.arena);
                    nid.append(prev_nid, &mut self.arena);
                } else {
                    // The left side binding power (lbp) of the current token is
                    // greater or equal to the previous token's right side binding
                    // power (rbp). Therefore, we must evaluate the current token
                    // first. The previous token becomes the parent of the current
                    // token.
                    // We take this path with the original assert as the previous
                    // token.
                    debug!("{} is child of {}", nid, prev_nid);
                    prev_nid.append(nid, &mut self.arena);
                }

                // Advance to the next token
                *tok_num += 1;
                result = self.parse_expr(tok_num, nid, diags, rbp);
            }
        } else {
            self.err_no_input(diags);
        }
        self.dbg_exit("parse_expr", result)
    }

    /// Parser for an assert statement
    /// We do not yet have full mathematical expression evaluation.
    /// The assert must be a 3 part expression with the middle lexical element
    /// either a == or !=.
    fn parse_assert(&mut self, tok_num: &mut usize, parent: NodeId,
                    diags: &mut Diags) -> bool {

        self.dbg_enter("parse_assert", *tok_num);
        // Add the assert keyword as a child of the parent
        let assert_nid = self.add_to_parent_and_advance(tok_num, parent);
        let mut result = self.parse_expr(tok_num, assert_nid, diags, 0);
        // we expect the current token to be a semicolon.
        if result {
            result = self.expect_semi(diags, tok_num, assert_nid);
        }

        self.dbg_exit("parse_assert", result)
    }

    /// Parses a numeric expression up to the next semicolon. Factors of the
    /// expression are attached as children of the parent nid
    fn parse_numeric(&mut self, tok_num : &mut usize, parent : NodeId,
                        diags: &mut Diags) -> bool {

        self.dbg_enter("parse_numeric", *tok_num);

        // A numeric expression must begin with an integer or function
        if let Some(tinfo) = self.tv.get(*tok_num) {
            match tinfo.tok {
                LexToken::Int => {
                    self.add_to_parent_and_advance(tok_num, parent);
                },
                _ => {
                    let m = format!("Invalid numeric expression '{}' was recognized as {:?}",
                                     tinfo.val, tinfo.tok);
                    diags.err1("AST_12", &m, self.tv[*tok_num].span());
                    return self.dbg_exit("parse_numeric", false);
                }
            }
        } else {
            self.err_no_input(diags);
            return self.dbg_exit("parse_numeric", false);
        }

        // After the initial numeric, the grammar allows zero or more pairs of
        // operator followed numeric until a semicolon
        let result = self.parse_op_numeric(tok_num, parent, diags);
        self.dbg_exit("parse_numeric", result)
    }

    /// Parses zero or more of 'operator followed by numeric' expressions.
    /// Recursion ends on an error or the first semicolon found. Zero operator
    /// numeric pairs is considered success and returns true.
    fn parse_op_numeric(&mut self, tok_num : &mut usize, parent : NodeId,
                        diags: &mut Diags) -> bool {

        self.dbg_enter("parse_op_numeric", *tok_num);

        // A numeric expression must begin with an integer or function
        if let Some(tinfo) = self.tv.get(*tok_num) {

            // first, expect an operator
            match tinfo.tok {
                LexToken::Semicolon => {
                    self.add_to_parent_and_advance(tok_num, parent);
                    return self.dbg_exit("parse_op_numeric", true);
                },
                LexToken::EqEq |
                LexToken::NEq => { self.add_to_parent_and_advance(tok_num, parent); },
                _ => {
                    // The caller may decide to skip to the next semicolon.
                    let m = format!("Invalid comparison operator '{}'", self.tv[*tok_num].val);
                    diags.err1("AST_11", &m, self.tv[*tok_num].span());
                    return self.dbg_exit("parse_op_numeric", false);
                }
            }

            // Now expect a numeric, so recurse
            let result = self.parse_numeric(tok_num, parent, diags);
            return self.dbg_exit("parse_op_numeric", result);
        }

        // We we get here, the loop ran out of input before finding a semicolon
        self.err_no_input(diags);
        return self.dbg_exit("parse_op_numeric", false);
    }

    fn parse_output(&mut self, tok_num : &mut usize, parent : NodeId,
                        diags: &mut Diags) -> bool {

        self.dbg_enter("parse_output", *tok_num);
        let mut result = false;
        // Add the section keyword as a child of the parent and advance
        let output_nid = self.add_to_parent_and_advance(tok_num, parent);

        // After 'output' a section identifier is expected
        if self.expect_leaf(diags, tok_num, output_nid, LexToken::Identifier, "AST_7",
                             "Expected a section name after output") {
            result = self.expect_semi(diags, tok_num, output_nid);
        }

        self.dbg_exit("parse_output", result)
    }

    /**
     * Adds the token as a child of teh parent and advances
     * the token index.
     */
    fn parse_leaf(&mut self, tok_num : &mut usize, parent : NodeId) {
        let tinfo = &self.tv[*tok_num]; // debug! only
        debug!("Ast::parse_leaf: Parsing token {}: {:?}", *tok_num, tinfo);
        let node = self.arena.new_node(*tok_num);
        parent.append(node, &mut self.arena);
        *tok_num += 1;
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
    pub fn dump(&self) -> anyhow::Result<()> {

        debug!("");

        let mut file = File::create("ast.dot").context(
            "Error attempting to create debug file 'ast.dot'")?;
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
        sections.insert(sec_str, Section::new(&ast,sec_nid));
        true
    }

    /// Returns true if the first child of the specified node is a section
    /// name that exists.  Otherwise, prints a diagnostic and returns false.
    fn validate_section_name(diags: &mut Diags, parent_nid: NodeId, ast: &'toks Ast,
                    sections: &HashMap<&'toks str, Section<'toks>> ) -> bool {
        debug!("AstDb::validate_section_name: NodeId {}", parent_nid);

        let mut children = parent_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
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
                if !Self::validate_section_name(diags, parent_nid, &ast, &sections) {
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
            },
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

        if !Self::validate_section_name(diags, output_nid, &ast, &sections) {
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
