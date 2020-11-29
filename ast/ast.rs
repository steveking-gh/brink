use logos::{Logos};
use indextree::{Arena,NodeId};
pub type Span = std::ops::Range<usize>;
use std::collections::HashMap;
use anyhow::{bail};
use diags::Diags;


#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

/// All tokens in roust created with the logos macro.
/// Keep this simple and do not be tempted to attach
/// unstructured values these enum.
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
        let toks_end = self.tv.len();
        debug!("Ast::parse: >>>> ENTER - Parsing {} tokens", toks_end);

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
        debug!("Ast::parse: <<<< EXIT({})", success);
        success
    }

    fn err_expected_after(&self, diags: &mut Diags, code: &str, msg: &str, tok_num: &usize) {
        let m = format!("{}, but found '{}'", msg, self.tv[*tok_num].val);
        diags.err2(code, &m, self.tv[*tok_num].span(), self.tv[*tok_num-1].span());
    }

    fn err_invalid_expression(&self, diags: &mut Diags, code: &str, tok_num: &usize) {
        let m = format!("Invalid expression '{}'", self.tv[*tok_num].val);
        diags.err1(code, &m, self.tv[*tok_num].span());
    }

    fn err_no_input(&self, diags: &mut Diags, tok_num: usize) {
        let m = format!("Unexpected end of input after '{}'", self.tv[tok_num].val);
        diags.err1("AST_13", &m, self.tv[tok_num].span());
    }

    fn err_no_close_brace(&self, diags: &mut Diags, brace_tok_num: usize) {
        let m = format!("Missing '}}'.  The following open brace is unmatched.");
        diags.err1("AST_14", &m, self.tv[brace_tok_num].span());
    }

    /// Get a token information object for the specified token number
    /// This is variant 1 since we have at least one other get_tinfo
    fn get_tinfo1(&self, tok_num: usize) -> Option<&'toks TokenInfo> {
        if tok_num >= self.tv.len() {
            return None;
        }

        Some(&self.tv[tok_num])
    }

    /// Attempts to advance the token number past the next semicolon
    /// The token number returned may be invalid.  This function is
    /// used to try to recover from syntax errors.
    fn advance_past_semicolon(&self, tok_num: usize) -> usize {
        let mut tnum = tok_num;
        while let Some(tinfo) = self.get_tinfo1(tnum) {
            tnum += 1;
            if tinfo.tok == LexToken::Semicolon {
                break;
            }
        }
        tnum
    }

    /// Add the specified token as a child of the parent
    /// Advance the token number and return the new node.
    fn add_to_parent_and_advance(&mut self, tok_num: &mut usize, parent: NodeId) -> NodeId {
        let nid = self.arena.new_node(*tok_num);
        parent.append(nid, &mut self.arena);
        *tok_num += 1;
        nid
    }

    fn expect_leaf(&mut self, diags: &mut Diags, tok_num : &mut usize,
        parent : NodeId, expected_token: LexToken, code: &str,
        context: &str) -> bool {

        if let Some(tinfo) = self.get_tinfo1(*tok_num) {
            if expected_token == tinfo.tok {
                debug!("Ast::expect_leaf: Parsing token {}: {:?}", *tok_num, tinfo);
                let node = self.arena.new_node(*tok_num);
                parent.append(node, &mut self.arena);
                *tok_num += 1;
            } else {
                self.err_expected_after(diags, code, context, tok_num);
                return false;
            }
        } else {
            self.err_no_input(diags, *tok_num - 1);
            return false;
        }
        true
    }

    fn parse_section(&mut self, tok_num : &mut usize, parent : NodeId,
                    diags: &mut Diags) -> bool {

        // Sections are always children of the root node, but no need to make
        // that a special case here.
        let sec_nid = self.add_to_parent_and_advance(tok_num, parent);

        // After 'section' an identifier is expected
        if !self.expect_leaf(diags, tok_num, sec_nid, LexToken::Identifier, "AST_1",
                             "Expected an identifier after section") {
            return false;
        }

        // After a section identifier, expect an open brace.
        // Remember the location of the opening brace to help with
        // user missing brace errors.
        let brace_toknum = *tok_num;
        if !self.expect_leaf(diags, tok_num, sec_nid, LexToken::OpenBrace, "AST_2",
                             "Expected { after identifier") {
            return false;
        }

        self.parse_section_contents(tok_num, sec_nid, diags, brace_toknum)
    }

    fn parse_section_contents(&mut self, tok_num : &mut usize, parent : NodeId,
                              diags: &mut Diags, brace_tok_num: usize) -> bool {

        let mut success = true;
        while let Some(tinfo) = self.get_tinfo1(*tok_num) {
            // When we find a close brace, we're done with section content
            if tinfo.tok == LexToken::CloseBrace {
                self.parse_leaf(tok_num, parent);
                return success;
            }

            // Stay in the section even after errors to give the user
            // more than one error at a time
            let parse_ok = match tinfo.tok {
                LexToken::Wrs => self.parse_wrs(tok_num, parent, diags),
                _ => {
                    self.err_invalid_expression(diags, "AST_3", tok_num);
                    *tok_num += 1;
                    false
                }
            };

            // If something went wrong, then advance to the next semi
            // and try to keep going to give users more errors to fix.
            if !parse_ok {
                *tok_num = self.advance_past_semicolon(*tok_num);
                success = false;
            }
        }

        // If we got here, we ran out of tokens before finding the close brace.
        self.err_no_close_brace(diags, brace_tok_num);
        false
    }

    fn parse_wrs(&mut self, tok_num : &mut usize, parent_nid : NodeId,
                diags: &mut Diags) -> bool {

        // Add the section keyword as a child of the parent and advance
        let wrs_nid = self.add_to_parent_and_advance(tok_num, parent_nid);

        // Next, a quoted string is expected
        if !self.expect_leaf(diags, tok_num, wrs_nid, LexToken::QuotedString, "AST_4",
                             "Expected a quoted string after 'wrs'") {
            return false;
        }

        // After the string, a semicolon
        if !self.expect_leaf(diags, tok_num, wrs_nid, LexToken::Semicolon, "AST_5",
                             "Expected ';' after string") {
            return false;
        }

        debug!("parse_wrs success");
        true
    }

    fn parse_output(&mut self, tok_num : &mut usize, parent : NodeId,
                        diags: &mut Diags) -> bool {

        // Add the section keyword as a child of the parent and advance
        let output_nid = self.add_to_parent_and_advance(tok_num, parent);

        // After 'output' a section identifier is expected
        if !self.expect_leaf(diags, tok_num, output_nid, LexToken::Identifier, "AST_7",
                             "Expected a section name after output") {
            return false;
        }

        // After the identifier, a semicolon
        if !self.expect_leaf(diags, tok_num, output_nid, LexToken::Semicolon, "AST_8",
                             "Expected ';' after identifier") {
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

    fn dump_r(&self, nid: NodeId, depth: usize) {
        debug!("AST: {}: {}{}", nid, " ".repeat(depth * 4), self.get_tinfo(nid).val);
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
        Section { tinfo: ast.get_tinfo(nid), nid }
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
    pub output: Option<Output<'toks>>,
    //pub properties: HashMap<NodeId, NodeProperty>
}

impl<'toks> AstDb<'toks> {

    /// Processes a section in the AST
    /// diags: the system context
    fn record_section(diags: &mut Diags, sec_nid: NodeId, ast: &'toks Ast,
                    sections: &mut HashMap<&'toks str, Section<'toks>> ) -> bool {
        debug!("AstDb::record_section: NodeId {}", sec_nid);

        // sec_nid points to 'section'
        // the first child of section is the section identifier
        // AST processing guarantees this exists, so unwrap
        let mut children = sec_nid.children(&ast.arena);
        let name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(name_nid);
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

    pub fn new(diags: &mut Diags, ast: &'toks Ast) -> anyhow::Result<AstDb<'toks>> {
        // Populate the AST database of critical structures.
        let mut result = true;

        let mut sections: HashMap<&'toks str, Section<'toks>> = HashMap::new();
        let mut output: Option<Output> = None;

        for nid in ast.root.children(&ast.arena) {
            let tinfo = ast.get_tinfo(nid);
            result = result && match tinfo.tok {
                LexToken::Section => Self::record_section(diags, nid, &ast, &mut sections),
                LexToken::Output => { output = Some(Output::new(&ast,nid)); true },
                _ => { true }
            };
        }

        if !result {
            bail!("AST construction failed");
        }

        Ok(AstDb { sections, output })
    }
}
