use logos::{Logos};
use indextree::{Arena,NodeId};
pub type Span = std::ops::Range<usize>;
use std::collections::HashMap;
use anyhow::{bail};
use diags::Diags;


#[allow(unused_imports)]
#[allow(unused_imports)]
use log::{error, warn, info, debug, trace};

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

#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo<'toks> {
    tok : LexToken,
    loc : Span,
    s : &'toks str,
}

impl<'toks> TokenInfo<'toks> {
    pub fn tok(&self) -> &LexToken { &self.tok }
    pub fn span(&self) -> Span { self.loc.clone() }
    pub fn slice(&self) -> &str { &self.s }
}

/**
 * Abstract Syntax Tree
 * This structure contains the AST created from the raw lexical
 * tokens.  The lifetime of this struct is the same as the tokens.
 */
pub struct Ast<'toks> {
    pub arena: Arena<usize>,
    pub tv: Vec<TokenInfo<'toks>>,
    pub root: NodeId,
}

impl<'toks> Ast<'toks> {
    pub fn new(fstr: &'toks str) -> Self {
        let mut arena = Arena::new();
        let root = arena.new_node(usize::MAX);
        let mut tv = Vec::new();
        let mut lex = LexToken::lexer(fstr);
        while let Some(t) = lex.next() {
            tv.push(TokenInfo{tok: t, s:lex.slice(), loc: lex.span()});
        }

        Self { arena, tv, root }
    }

    pub fn parse(&mut self, diags: &mut Diags) -> bool {
        let toks_end = self.tv.len();
        let mut tok_num = 0;

        // We can't simply iterate on the token vector, since we consume
        // tokens from the vector recursively in varying amounts.
        while tok_num < toks_end {
            let tinfo = &self.tv[tok_num];
            debug!("Ast::parse: Parsing token {}: {:?}", &mut tok_num, tinfo);
            let success = match tinfo.tok() {
                LexToken::Section => self.parse_section(&mut tok_num, self.root, diags),
                LexToken::Output => self.parse_output(&mut tok_num, self.root, diags),
                _ => false,
            };
        }
        true
    }

    fn err_expected_after(&self, diags: &mut Diags, code: i32, msg: &str, tok_num: &usize) {
        let m = format!("{}, but found '{}'", msg, self.tv[*tok_num].slice());
        diags.err2(code, &m, self.tv[*tok_num].span(), self.tv[*tok_num-1].span());
    }

    fn err_invalid_expression(&self, diags: &mut Diags, code: i32, tok_num: &usize) {
        let m = format!("Invalid expression '{}'", self.tv[*tok_num].slice());
        diags.err1(code, &m, self.tv[*tok_num].span());
    }

    /// Add the specified token as a child of the parent
    /// Advance the token number and return the new node.
    fn add_to_parent_and_advance(&mut self, tok_num: &mut usize, parent: NodeId) -> NodeId {
        let nid = self.arena.new_node(*tok_num);
        parent.append(nid, &mut self.arena);
        *tok_num += 1;
        nid
    }

    fn parse_section(&mut self, tok_num : &mut usize, parent : NodeId,
                    diags: &mut Diags) -> bool {

        // Add the section keyword as a child of the parent and advance
        let node = self.add_to_parent_and_advance(tok_num, parent);

        // After a section declaration, an identifier is expected
        let tinfo = &self.tv[*tok_num];
        if let LexToken::Identifier = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            let m = format!("Expected an identifier after 'section', but found '{}'",
                            self.tv[*tok_num].slice());
            diags.err2(1, &m, self.tv[*tok_num].span(), self.tv[*tok_num-1].span());
            return false;
        }

        // After a section identifier, open brace
        let tinfo = &self.tv[*tok_num];
        if let LexToken::OpenBrace = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            let m = format!("Expected {{ after identifier, but found '{}'",
                            self.tv[*tok_num].slice());
            diags.err2(2, &m, self.tv[*tok_num].span(), self.tv[*tok_num-1].span());
            return false;
        }

        self.parse_section_contents(tok_num, node, diags);
        true
    }

    fn parse_section_contents(&mut self, tok_num : &mut usize, parent : NodeId,
                                        diags: &mut Diags) -> bool {
        let toks_end = self.tv.len();
        while *tok_num < toks_end {
            let tinfo = &self.tv[*tok_num];
            match tinfo.tok() {
                // For now, we only support writing strings in a section.
                LexToken::Wrs => {
                    if !self.parse_wrs(tok_num, parent, diags) {
                        return false;
                    }
                }
                LexToken::CloseBrace => {
                    // When we find a close brace, we're done with section content
                    self.parse_leaf(tok_num, parent);
                    return true;
                }
                _ => {
                    self.err_invalid_expression(diags, 3, tok_num);
                    return false;
                }
            }
        }
        true
    }

    fn parse_wrs(&mut self, tok_num : &mut usize, parent : NodeId,
                diags: &mut Diags) -> bool {

        // Add the section keyword as a child of the parent and advance
        let node = self.add_to_parent_and_advance(tok_num, parent);

        // Next, a quoted string is expected
        let tinfo = &self.tv[*tok_num];
        if let LexToken::QuotedString = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            self.err_expected_after(diags, 4, "Expected a quoted string after 'wrs'", tok_num);
            return false;
        }

        // Finally a semicolon
        let tinfo = &self.tv[*tok_num];
        if let LexToken::Semicolon = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            self.err_expected_after(diags, 5, "Expected ';' after string", tok_num);
            return false;
        }
        debug!("parse_wrs success");
        true
    }

    fn parse_output(&mut self, tok_num : &mut usize, parent : NodeId,
                        diags: &mut Diags) -> bool {

        // Add the section keyword as a child of the parent and advance
        let node = self.add_to_parent_and_advance(tok_num, parent);

        // After a output declaration we expect a section identifier
        let tinfo = &self.tv[*tok_num];
        if let LexToken::Identifier = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            self.err_expected_after(diags, 7, "Expected a section name after output", tok_num);
            return false;
        }

        // After the identifier, the file name as a quoted string
        let tinfo = &self.tv[*tok_num];
        if let LexToken::QuotedString = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            self.err_expected_after(diags, 6, "Expected the file path as a quoted string after the section name", tok_num);
            return false;
        }

        // After the identifier, a semicolon
        let tinfo = &self.tv[*tok_num];
        if let LexToken::Semicolon = tinfo.tok() {
            self.parse_leaf(tok_num, node);
        } else {
            self.err_expected_after(diags, 8, "Expected ';' after identifier", tok_num);
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
        debug!("AST: {}: {}{}", nid, " ".repeat(depth * 4), self.get_tinfo(nid).slice());
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
        let sec_str = sec_tinfo.slice();
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
    pub outputs: Vec<Output<'toks>>,
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
        let sec_str = sec_tinfo.slice();
        if sections.contains_key(sec_str) {
            // error, duplicate section names
            // We know the section exists, so unwrap is fine.
            let orig_section = sections.get(sec_str).unwrap();
            let orig_tinfo = orig_section.tinfo;
            let m = format!("Duplicate section name '{}'", sec_str);
            diags.err2(9, &m, sec_tinfo.span(), orig_tinfo.span());
            return false;
        }
        sections.insert(sec_str, Section::new(&ast,sec_nid));
        true
    }

    /**
     * Adds a new output to the vector of output structs.
     */
    fn record_output(_ctxt: &mut Diags, nid: NodeId, ast: &'toks Ast,
                    outputs: &mut Vec<Output<'toks>>) -> bool {
        // nid points to 'output'
        // don't bother with semantic error checking yet.
        // The lexer already did basic checking
        debug!("AstDb::record_output: NodeId {}", nid);
        outputs.push(Output::new(&ast, nid));
        true
    }

    pub fn new(diags: &mut Diags, ast: &'toks Ast) -> anyhow::Result<AstDb<'toks>> {
        // Populate the AST database of critical structures.
        let mut result = true;

        let mut sections: HashMap<&'toks str, Section<'toks>> = HashMap::new();
        let mut outputs: Vec<Output<'toks>> = Vec::new();

        for nid in ast.root.children(&ast.arena) {
            let tinfo = ast.get_tinfo(nid);
            result = result && match tinfo.tok() {
                LexToken::Section => Self::record_section(diags, nid, &ast, &mut sections),
                LexToken::Output => Self::record_output(diags, nid, &ast, &mut outputs),
                _ => { true }
            };
        }

        if !result {
            bail!("AST construction failed");
        }

        Ok(AstDb { sections, outputs })
    }
}
