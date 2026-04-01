// Lexer, parser and abstract syntax tree (AST) for brink.
//
// This is the first stage of the compiler pipeline.  The logos-generated
// lexer converts the raw source text into a flat token stream (LexToken).
// The recursive-descent / Pratt-expression parser then consumes that stream
// and builds an indextree arena-based AST, where each node holds a TokenInfo
// that records the token kind, its string value, and its byte-offset span in
// the source file.  A second pass, AstDb, validates global constraints such as
// unique section names and resolves cross-section references.
//
// Order of operations: ast runs immediately after the source file is read.
// Its output — an Ast and an AstDb — is consumed by lineardb in the next
// stage.

use anyhow::{Context, bail};
use diags::{Diags, SourceSpan};
use indextree::{Arena, NodeId};
use logos::Logos;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::prelude::*;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// All tokens in brink created with the logos macro.
/// Keep this simple and do not be tempted to attach
/// unstructured values these enum.
#[derive(Logos, Debug, Clone, Copy, PartialEq)]
pub enum LexToken {
    #[token("const")]
    Const,
    // Built-in variables — must be listed before the Identifier regex so that
    // logos gives them priority over the generic identifier pattern.
    #[token("__OUTPUT_SIZE")]
    OutputSize,
    #[token("__OUTPUT_ADDR")]
    OutputAddr,
    #[token("section")]
    Section,
    #[token("align")]
    Align,
    #[token("set_sec_offset")]
    SetSecOffset,
    #[token("set_addr_offset")]
    SetAddrOffset,
    #[token("set_addr")]
    SetAddr,
    #[token("set_file_offset")]
    SetFileOffset,
    #[token("assert")]
    Assert,
    #[token("sizeof")]
    Sizeof,
    #[token("print")]
    Print,
    #[token("to_u64")]
    ToU64,
    #[token("to_i64")]
    ToI64,
    #[token("addr")]
    Addr,
    #[token("addr_offset")]
    AddrOffset,
    #[token("sec_offset")]
    SecOffset,
    #[token("file_offset")]
    FileOffset,
    #[token("wrs")]
    Wrs,
    #[token("wr8")]
    Wr8,
    #[token("wr16")]
    Wr16,
    #[token("wr24")]
    Wr24,
    #[token("wr32")]
    Wr32,
    #[token("wr40")]
    Wr40,
    #[token("wr48")]
    Wr48,
    #[token("wr56")]
    Wr56,
    #[token("wr64")]
    Wr64,
    #[token("wrf")]
    Wrf,
    #[token("wr")]
    Wr,
    #[token("output")]
    Output,
    #[token("==")]
    DoubleEq,
    #[token("!=")]
    NEq,
    #[token(">=")]
    GEq,
    #[token("<=")]
    LEq,
    // Single '=' must be lexed after all the multi-character operators
    // that start with '=' to avoid ambiguity.
    #[token("=")]
    Eq,
    #[token("&&")]
    DoubleAmpersand,
    #[token("||")]
    DoublePipe,
    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Asterisk,
    #[token("/")]
    FSlash,
    #[token("%")]
    Percent,
    #[token(",")]
    Comma,
    #[token("<<")]
    DoubleLess,
    #[token(">>")]
    DoubleGreater,
    #[token("{")]
    OpenBrace,
    #[token("}")]
    CloseBrace,
    #[token("(")]
    OpenParen,
    #[token(")")]
    CloseParen,
    #[token(";")]
    Semicolon,
    #[regex("[_a-zA-Z][0-9a-zA-Z_]*:")]
    Label,
    #[regex("[_a-zA-Z][0-9a-zA-Z_]*::")]
    Namespace,
    #[regex("[_a-zA-Z][0-9a-zA-Z_]*")]
    Identifier,

    // Plain vanilla numbers that are ambiguously signed or unsigned
    #[regex("[1-9][_0-9]*|0")]
    Integer,

    // Unsigned literals are suffixed with 'u'
    // binary and hex numbers are unsigned by default and don't require u suffix
    #[regex("0[bB][01][_01]*u?|0[xX][0-9a-fA-F][_0-9a-fA-F]*u?|[1-9][_0-9]*u|0u")]
    U64,

    // Signed literals are suffixed with 'i' and/or start with a minus sign
    #[regex("0[bB][01][_01]*i|0[xX][0-9a-fA-F][_0-9a-fA-F]*i|[1-9][_0-9]*i|-[1-9][_0-9]*i?|0i")]
    I64,

    // Not only is \ special in strings and must be escaped, but also special in
    // regex.  We use raw string here to avoid having the escape the \ for the
    // string itself. The \\ in this raw string are escape \ for the regex
    // engine underneath.
    #[regex(r#""(\\"|\\.|[^"])*""#)]
    QuotedString,

    // Comments and whitespace are stripped from user input during processing.
    // This stripping happens *after* we record all the line/offset info
    // with codespan for error reporting.
    #[regex(r#"/\*([^*]|\*[^/])+\*/"#, logos::skip)] // block comments
    #[regex(r#"//[^\r\n]*(\r\n|\n)?"#, logos::skip)] // line comments
    #[regex(r#"[ \t\n\f]+"#, logos::skip)] // whitespace
    #[error]
    Unknown,
}

/// Returns true if `name` is a reserved identifier that may not be used
/// as a section name, const name, or label name.
///
/// Reserved prefixes:
///   - "wr" + digit  — write instructions (wr8, wr16, wr32, etc)
///   - "set_"        — configuration directives (set_sec_offset, set_addr, etc)
///   - "__"          — leading double underscore names refer to builtin identifiers
///
/// Reserved exact keywords:
///   - "wrs" / "wrf"              — write-string and write-file commands
///   - "include" / "import"       — file or module inclusion
///   - "if" / "else"              — conditional section inclusion
///   - "true" / "false"           — boolean literals
///   - "extern"                   — external section references
///   - "let"                      — future variable declarations
///   - "fill"                     — fill/pad byte ranges
pub fn is_reserved_identifier(name: &str) -> bool {
    // "wr" followed by at least one digit reserves the numeric write variants.
    if let Some(rest) = name.strip_prefix("wr") {
        if rest.starts_with(|c: char| c.is_ascii_digit()) {
            return true;
        }
    }
    if name.starts_with("set_") || name.starts_with("__") {
        return true;
    }
    matches!(
        name,
        "wrs"
            | "wrf"
            | "include"
            | "import"
            | "if"
            | "else"
            | "true"
            | "false"
            | "extern"
            | "let"
            | "fill"
    )
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
    pub loc: SourceSpan,

    /// The value of the token trimmed of whitespace
    pub val: &'toks str,
}

impl<'toks> TokenInfo<'toks> {
    pub fn span(&self) -> SourceSpan {
        self.loc.clone()
    }
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

    /// Recursively lexes the provided `fstr` source string, flattening the tokens directly
    /// into the main `tv` stream. Encountering an `include "path";` sequence triggers
    /// path resolution relative to the current file, reading the target contents,
    /// registering the path with `diags` to claim a new `file_id`, and recursing
    /// to insert the nested tokens into the stream.
    ///
    /// The `visited` hash map tracks canonical paths to prevent include cycles.
    fn lex_file_r(
        tv: &mut Vec<TokenInfo<'toks>>,
        name: &str,
        fstr: &'toks str,
        file_id: usize,
        diags: &mut Diags,
        visited: &mut HashMap<String, SourceSpan>,
    ) -> anyhow::Result<()> {
        let mut lex = LexToken::lexer(fstr);
        while let Some(tok) = lex.next() {
            let val = lex.slice();
            let span = lex.span();
            if tok == LexToken::Identifier && val == "include" {
                // Determine whether 'include' serves as a top-level directive
                // rather than a misused reserved identifier (e.g., `const include = ...`).
                // Checking whether 'include'appears immediately after a statement boundary (or at the
                // start of the file) prevents eagerly intercepting valid parser-level error
                // cases like AST_32 (Reserved section name) or AST_33 (Reserved const name).
                let is_directive = tv
                    .last()
                    .is_none_or(|t| matches!(t.tok, LexToken::Semicolon | LexToken::CloseBrace));

                if is_directive {
                    let next_tok = lex.next();
                    if next_tok != Some(LexToken::QuotedString) {
                        diags.err1(
                            "AST_34",
                            "Expected quoted string after include",
                            SourceSpan {
                                file_id,
                                range: span,
                            },
                        );
                        anyhow::bail!("AST lexing failed");
                    }
                    let path_val = lex.slice();
                    let path_span = lex.span();

                    let semi_tok = lex.next();
                    if semi_tok != Some(LexToken::Semicolon) {
                        diags.err1(
                            "AST_35",
                            "Expected semicolon after include statement",
                            SourceSpan {
                                file_id,
                                range: path_span,
                            },
                        );
                        anyhow::bail!("AST lexing failed");
                    }

                    let raw_path = path_val
                        .strip_prefix('"')
                        .unwrap()
                        .strip_suffix('"')
                        .unwrap();
                    let base_dir = std::path::Path::new(name)
                        .parent()
                        .unwrap_or(std::path::Path::new(""));
                    let resolved_path = base_dir.join(raw_path);

                    // Use a normalized string path for cycle detection
                    let resolved_path_str = if let Ok(c) = resolved_path.canonicalize() {
                        c.to_string_lossy().to_string()
                    } else {
                        // Fallback to basic string if it doesn't exist to generate good error
                        resolved_path.to_string_lossy().to_string()
                    };

                    if let Some(orig_span) = visited.get(&resolved_path_str) {
                        diags.err2(
                            "AST_36",
                            &format!("Include cycle detected: {}", resolved_path_str),
                            SourceSpan {
                                file_id,
                                range: span.clone(),
                            },
                            orig_span.clone(),
                        );
                        anyhow::bail!("AST lexing failed");
                    }
                    visited.insert(
                        resolved_path_str.clone(),
                        SourceSpan {
                            file_id,
                            range: span.clone(),
                        },
                    );

                    let content = match std::fs::read_to_string(&resolved_path) {
                        Ok(c) => c,
                        Err(e) => {
                            diags.err1(
                                "AST_37",
                                &format!(
                                    "Failed to read included file '{}': {}",
                                    resolved_path_str, e
                                ),
                                SourceSpan {
                                    file_id,
                                    range: path_span,
                                },
                            );
                            anyhow::bail!("AST lexing failed");
                        }
                    };

                    let leaked_content: &'toks str = Box::leak(content.into_boxed_str());
                    let inc_file_id = diags.add_file(&resolved_path_str, leaked_content);

                    Self::lex_file_r(
                        tv,
                        &resolved_path_str,
                        leaked_content,
                        inc_file_id,
                        diags,
                        visited,
                    )?;

                    continue;
                }
            }

            debug!("ast::new: Token {} = {:?}", tv.len(), tok);
            tv.push(TokenInfo {
                tok,
                val,
                loc: SourceSpan {
                    file_id,
                    range: span,
                },
            });
        }
        Ok(())
    }

    /// Create a new abstract syntax tree.
    pub fn new(name: &str, fstr: &'toks str, diags: &mut Diags) -> anyhow::Result<Self> {
        let mut arena = Arena::new();
        let root = arena.new_node(usize::MAX);
        let mut tv = Vec::new();
        let mut visited = HashMap::new();

        // In Phase 1, `process.rs` adds the main file to diags at id=0.
        // We reuse that knowledge here.
        let main_path = if let Ok(c) = std::path::Path::new(name).canonicalize() {
            c.to_string_lossy().to_string()
        } else {
            name.to_string()
        };
        visited.insert(
            main_path,
            SourceSpan {
                file_id: 0,
                range: 0..0,
            },
        );

        Self::lex_file_r(&mut tv, name, fstr, 0, diags, &mut visited)?;
        let mut ast = Self {
            arena,
            tv,
            root,
            tok_num: 0,
        };
        if !ast.parse(diags) {
            // ast construction failed.  Let the caller report
            // this in whatever way they want.
            anyhow::bail!("AST construction failed.");
        }

        Ok(ast)
    }

    // Boilerplate entry debug tracing for recursive descent parsing functions.
    fn dbg_enter(&self, func_name: &str) {
        if let Some(tinfo) = self.peek() {
            trace!(
                "Ast::{} ENTER, {}:{} is {:?}",
                func_name, self.tok_num, tinfo.val, tinfo.tok
            );
        } else {
            trace!(
                "Ast::{} ENTER, {}:{} is {}",
                func_name, self.tok_num, "<end of input>", "<end of input>"
            );
        }
    }

    /// Boilerplate exit debug tracing for recursive descent parsing functions.
    /// This function returns the result and should be the last statement
    /// in each function
    fn dbg_exit(&self, func_name: &str, result: bool) -> bool {
        trace!("Ast::{} EXIT {:?}", func_name, result);
        result
    }

    /// Boilerplate exit debug tracing for pratt parsing functions.
    /// This function returns the result and should be the last statement
    /// in each function
    fn dbg_exit_pratt(&self, func_name: &str, top: &Option<NodeId>, result: bool) -> bool {
        trace!(
            "Ast::{} EXIT {} with node id {:?}",
            func_name,
            if result { "OK" } else { "!! FAIL !!" },
            top
        );
        result
    }

    /// Return an iterator over the children of the specified AST node
    pub fn children(&self, nid: NodeId) -> indextree::Children<'_, usize> {
        nid.children(&self.arena)
    }

    /// Returns true if the specified node has child nodes
    pub fn has_children(&self, nid: NodeId) -> bool {
        nid.children(&self.arena).next().is_some()
    }

    /// Returns the lexical value of the specified child of the specified
    /// parent. The value is always a string reference to source code regardless
    /// of the semantic meaning of the child.
    pub fn get_child_str(&'toks self, parent_nid: NodeId, child_num: usize) -> Option<&'toks str> {
        debug!(
            "Ast::get_child_str: child number {} for parent nid {}",
            child_num, parent_nid
        );
        let mut children = parent_nid.children(&self.arena);
        if let Some(name_nid) = children.nth(child_num) {
            let tinfo = self.get_tinfo(name_nid);
            return Some(tinfo.val);
        }
        None
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
                LexToken::Const => self.parse_const(self.root, diags),
                LexToken::Assert => {
                    // Global assert: evaluated in the validation phase after all
                    // sections and extensions are written.
                    let ok = self.parse_expr(self.root, diags);
                    if !ok {
                        self.advance_past_semicolon();
                    }
                    ok
                }

                // Unrecognized top level token.  Report the error, but keep going
                // to try to give the user more errors in batches.
                _ => {
                    let msg = format!("Unrecognized token '{}' at top level scope", tinfo.val);
                    diags.err1("AST_18", &msg, tinfo.span());

                    // Skip the bad token.
                    self.tok_num += 1;
                    false
                }
            };
        }
        self.dbg_exit("parse", result)
    }

    fn err_expected_after(&self, diags: &mut Diags, code: &str, msg: &str) {
        let m = format!("{}, but found '{}'", msg, self.tv[self.tok_num].val);
        diags.err2(
            code,
            &m,
            self.tv[self.tok_num].span(),
            self.tv[self.tok_num - 1].span(),
        );
    }

    fn err_invalid_expression(&self, diags: &mut Diags, code: &str) {
        let m = format!("Invalid expression '{}'", self.tv[self.tok_num].val);
        diags.err1(code, &m, self.tv[self.tok_num].span());
    }

    fn err_no_input(&self, diags: &mut Diags) {
        diags.err0("AST_13", "Unexpected end of input");
    }

    fn err_no_close_brace(&self, diags: &mut Diags, brace_tok_num: usize) {
        let m = "Missing '}'.  The following open brace is unmatched.".to_string();
        diags.err1("AST_14", &m, self.tv[brace_tok_num].span());
    }

    /// Attempts to advance the token number past the next semicolon. The final
    /// token number may be invalid.  This function is used to try to recover
    /// from syntax errors.
    ///
    /// If the current token is already one past a semicolon, then do nothing.
    /// This case occurs when the semicolon itself was unexpected, e.g. missing
    /// close paren like assert(1;
    fn advance_past_semicolon(&mut self) {
        assert!(self.tok_num > 0);
        self.dbg_enter("advance_past_semicolon");
        if let Some(prev_tinfo) = self.tv.get(self.tok_num - 1)
            && prev_tinfo.tok != LexToken::Semicolon
        {
            while let Some(tinfo) = self.take() {
                if tinfo.tok == LexToken::Semicolon {
                    break;
                }
            }
        }
        debug!(
            "Ast::advance_past_semicolon: Stopped on token {}",
            self.tok_num
        );
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

    fn expect_leaf(
        &mut self,
        diags: &mut Diags,
        parent: NodeId,
        expected_token: LexToken,
        code: &str,
        context: &str,
    ) -> bool {
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
    fn expect_semi(&mut self, diags: &mut Diags, parent: NodeId) -> bool {
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
    fn expect_token(&mut self, tok: LexToken, diags: &mut Diags, parent: NodeId) -> bool {
        if let Some(tinfo) = self.peek() {
            if tok == tinfo.tok {
                self.add_to_parent_and_advance(parent);
                return true;
            } else {
                let msg = format!("Expected {:?}", tok);
                self.err_expected_after(diags, "AST_26", &msg);
            }
        } else {
            self.err_no_input(diags);
        }

        false
    }

    /// Expect an expression which cannot be None.
    fn expect_expr(&mut self, parent: NodeId, diags: &mut Diags) -> bool {
        let mut expr_opt = None;
        if !self.parse_pratt(0, &mut expr_opt, diags) {
            return false;
        }
        if expr_opt.is_none() {
            let tinfo = self.get_tinfo(parent);
            let msg = format!(
                "Expected valid expression inside parentheses after {:?}",
                tinfo.tok
            );
            diags.err1("AST_12", &msg, tinfo.span());
            return false;
        }
        // Success, add the expression a child of the input parent node.
        parent.append(expr_opt.unwrap(), &mut self.arena);
        true
    }

    /// Expect zero or one instance of specified tokens.
    /// If we find an allowed found, add it to the parent and advance.
    /// If not found, do nothing and return success
    fn optional_token(&mut self, tokvec: &[LexToken], diags: &mut Diags, parent: NodeId) -> bool {
        if let Some(tinfo) = self.peek() {
            if tokvec.contains(&tinfo.tok) {
                self.add_to_parent_and_advance(parent);
            }
        } else {
            self.err_no_input(diags);
        }

        true
    }

    /// Expect the specified token and advance without adding to the parent.
    // TODO reorder parameters so diags is last
    fn expect_token_no_add(&mut self, tok: LexToken, diags: &mut Diags) -> bool {
        if let Some(tinfo) = self.peek() {
            if tok == tinfo.tok {
                self.tok_num += 1;
                return true;
            } else {
                let msg = format!("Expected {:?}", tok);
                self.err_expected_after(diags, "AST_20", &msg);
            }
        } else {
            self.err_no_input(diags);
        }

        false
    }

    /// Parses a `section` declaration and attaches it to the AST.
    ///
    /// ```text
    /// section <name> { <statements> }
    ///
    ///   section              <- root node for a section declaration
    ///   ├── <Identifier>     <- section name
    ///   ├── {                <- syntactic delimiter, marks start of body
    ///   ├── [statements...]  <- zero or more content nodes (see parse_section_contents)
    ///   └── }                <- syntactic delimiter, marks end of body
    /// ```
    fn parse_section(&mut self, parent: NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_section");
        let mut result = false;
        // Sections are always children of the root node, but no need to make
        // that a special case here.
        let sec_nid = self.add_to_parent_and_advance(parent);

        // After 'section' an identifier is expected
        if self.expect_leaf(
            diags,
            sec_nid,
            LexToken::Identifier,
            "AST_1",
            "Expected an identifier after section",
        ) {
            // After a section identifier, expect an open brace.
            // Remember the location of the opening brace to help with
            // user missing brace errors.
            let brace_toknum = self.tok_num;
            if self.expect_leaf(
                diags,
                sec_nid,
                LexToken::OpenBrace,
                "AST_2",
                "Expected { after identifier",
            ) {
                result = self.parse_section_contents(sec_nid, diags, brace_toknum);
            }
        }
        self.dbg_exit("parse_section", result)
    }

    /// Parses the body of a section, appending statement nodes directly to the
    /// parent section node.  Loops until a `}` is found or tokens are exhausted.
    /// Each iteration dispatches to `parse_label`, `parse_wr`, or `parse_expr`
    /// depending on the leading token; unrecognized tokens produce a diagnostic
    /// and are skipped to the next `;` to allow recovery.
    ///
    /// ```text
    /// wr8 1+2; assert x; }
    ///
    ///   section              <- parent node (owned by parse_section)
    ///   ...
    ///   ├── <statement>      <- one node per statement in the section body
    ///   ├── <statement>
    ///   └── }                <- close-brace leaf, signals end of body
    /// ```
    fn parse_section_contents(
        &mut self,
        parent: NodeId,
        diags: &mut Diags,
        brace_tok_num: usize,
    ) -> bool {
        self.dbg_enter("parse_section_contents");
        let mut result = true; // todo fixme

        let mut tok_num_old = 0;
        while let Some(tinfo) = self.peek() {
            debug!(
                "Ast::parse_section_contents: token {}:{}",
                self.tok_num, tinfo.val
            );
            if tok_num_old == self.tok_num {
                // In some error cases, such as a missing closing brace, parsing
                // can get stuck without advancing the token pointer.  For
                // example, this problem occurs because an error occurs at the
                // very start of a new expression.  The advance_past_semicolon
                // function won't move us forward since we're already past a
                // semicolon and at the start of a new statement. As a simple
                // solution, detect that we're not making forward progress and
                // force the token number forward.
                self.tok_num += 1;
                debug!("parse_section_contents: Forcing forward progress.");
                continue;
            }
            tok_num_old = self.tok_num;
            // When we find a close brace, we're done with section content
            if tinfo.tok == LexToken::CloseBrace {
                self.parse_leaf(parent);
                return self.dbg_exit("parse_section_contents", result);
            }

            // Stay in the section even after errors to give the user
            // more than one error at a time
            let parse_ok = match tinfo.tok {
                LexToken::Label => self.parse_label(parent, diags),
                LexToken::Wr => self.parse_wr(parent, diags),
                LexToken::Wrf
                | LexToken::Wr8
                | LexToken::Wr16
                | LexToken::Wr24
                | LexToken::Wr32
                | LexToken::Wr40
                | LexToken::Wr48
                | LexToken::Wr56
                | LexToken::Wr64
                | LexToken::Wrs
                | LexToken::Assert
                | LexToken::Align
                | LexToken::SetSecOffset
                | LexToken::SetAddrOffset
                | LexToken::SetAddr
                | LexToken::SetFileOffset
                | LexToken::Print => self.parse_expr(parent, diags),
                _ => {
                    self.err_invalid_expression(diags, "AST_3");
                    false
                }
            };

            if !parse_ok {
                debug!(
                    "Ast::parse_section_contents: skipping to next ; starting from {}",
                    self.tok_num
                );
                // Consume the bad token and skip forward
                self.advance_past_semicolon();
                result = false;
            }
        }

        // If we got here, we ran out of tokens before finding the close brace.
        self.err_no_close_brace(diags, brace_tok_num);
        self.dbg_exit("parse_section_contents", false)
    }

    /// Parses a `wr` statement that copies a named section into the output
    /// or evaluates an extension call.
    ///
    /// ```text
    /// wr <name>;
    ///
    ///   wr               <- root node for a section-write statement
    ///   └── <Identifier> <- name of the section to be written
    ///
    /// or
    ///
    /// wr <namespace::extension_name>(<args>);
    ///
    ///   wr               <- root node for an extension call
    ///   └── <namespace::extension_name>(<args>) <- extension call
    /// ```
    fn parse_wr(&mut self, parent_nid: NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_wr");
        let mut result = false;

        // Add the wr keyword as a child of the parent and advance
        let wr_nid = self.add_to_parent_and_advance(parent_nid);

        // Next, an expression is expected
        if self.expect_expr(wr_nid, diags) {
            result = self.expect_semi(diags, wr_nid);
        }
        self.dbg_exit("parse_wr", result)
    }

    /// Returns the (lhs, rhs) binding power for any infix token.
    /// Higher numbers are stronger binding. Returns None if the
    /// token is not a valid infix operator.
    fn get_infix_binding_power(tok: LexToken) -> Option<(u8, u8)> {
        match tok {
            LexToken::Percent | LexToken::FSlash | LexToken::Asterisk => Some((19, 20)),
            LexToken::Minus | LexToken::Plus => Some((17, 18)),
            LexToken::DoubleLess | LexToken::DoubleGreater => Some((15, 16)),
            LexToken::Ampersand => Some((13, 14)),
            LexToken::Pipe => Some((11, 12)),
            LexToken::DoubleEq | LexToken::NEq | LexToken::LEq | LexToken::GEq => Some((9, 10)),
            LexToken::DoubleAmpersand => Some((7, 8)),
            LexToken::DoublePipe => Some((5, 6)),
            _ => None,
        }
    }

    /// Parses an expression with correct operator precedence using a Pratt
    /// (precedence-climbing) algorithm.  Returns the root `NodeId` of the
    /// sub-tree via `top`, or `None` if the expression is empty.  On success
    /// the terminal `;`, `,`, or `)` remains as the next unprocessed token.
    /// See <https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html>.
    ///
    /// ```text
    /// 1 + 2 * 3
    ///
    ///   +                <- root is the lowest-precedence operator
    ///   ├── 1            <- left atom
    ///   └── *            <- higher-precedence sub-expression
    ///       ├── 2
    ///       └── 3
    ///
    /// sizeof(<name>)
    ///
    ///   sizeof
    ///   └── <Identifier> <- mandatory section or label name
    ///
    /// abs([<name>])
    ///
    ///   abs
    ///   └── [<Identifier>] <- optional section or label name
    /// ```
    fn parse_pratt(&mut self, min_bp: u8, top: &mut Option<NodeId>, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_pratt");
        debug!("Ast::parse_pratt: Min BP = {}", min_bp);
        let lhs_tinfo = self.peek();
        if lhs_tinfo.is_none() {
            self.err_no_input(diags);
            return self.dbg_exit_pratt("parse_pratt", &None, false);
        }

        let lhs_tinfo = lhs_tinfo.unwrap();

        *top = None; // Initialize

        match lhs_tinfo.tok {
            // Finding a close paren or a semi-colon terminates an expression.
            LexToken::CloseParen | LexToken::Semicolon => {
                /* top will be None */
                *top = None;
            }

            // This open paren is precedence control in an expression, e.g. (1+2)*3.
            // This is not an open paren associated with a built-in function.
            LexToken::OpenParen => {
                // move past the open paren without storing in the AST.
                self.tok_num += 1;
                // lhs is everything inside parentheses.
                if !self.parse_pratt(0, top, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
                // Open paren must have a matching close paren.
                if !self.expect_token_no_add(LexToken::CloseParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
            }

            // These simple atoms end up as leaf nodes in the AST
            LexToken::QuotedString | LexToken::Integer | LexToken::I64 | LexToken::U64 => {
                *top = Some(self.arena.new_node(self.tok_num));
                self.tok_num += 1;
            }

            // A namespace component like `custom::` signals the start of a namespaced path.
            // This token must be immediately followed by a trailing identifier. If an open parenthesis
            // `(` follows the identifier, the parser aggregates the tokens into a generic
            // function invocation (e.g., `custom::foo(arg1, arg2)`).
            LexToken::Namespace => {
                let ns_nid = self.arena.new_node(self.tok_num);
                *top = Some(ns_nid);
                self.tok_num += 1;

                // A namespace prefix must immediately be followed by an identifier.
                let Some(next_tinfo) = self.peek() else {
                    self.err_no_input(diags);
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                };

                // Add the trailing identifier as the first child of the namespace node.
                if next_tinfo.tok == LexToken::Identifier {
                    let id_nid = self.arena.new_node(self.tok_num);
                    self.tok_num += 1;
                    ns_nid.append(id_nid, &mut self.arena);
                } else {
                    diags.err1(
                        "AST_39",
                        "Expected identifier after namespace",
                        next_tinfo.span(),
                    );
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }

                // If an open parenthesis follows, we parse this as a function invocation.
                if let Some(after_tinfo) = self.peek()
                    && after_tinfo.tok == LexToken::OpenParen
                {
                    self.tok_num += 1; // consume '('

                    loop {
                        let Some(check_tinfo) = self.peek() else {
                            self.err_no_input(diags);
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        };

                        // A trailing close parenthesis indicates the end of the argument list.
                        if check_tinfo.tok == LexToken::CloseParen {
                            self.tok_num += 1; // consume ')'
                            break;
                        }

                        // Recursively parse the next argument within the parenthesis.
                        let mut arg_opt = None;
                        if !self.parse_pratt(0, &mut arg_opt, diags) {
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        }
                        if let Some(arg_nid) = arg_opt {
                            ns_nid.append(arg_nid, &mut self.arena);
                        }

                        // Arguments must be separated by commas or terminated by a close parenthesis.
                        let Some(delim_tinfo) = self.peek() else {
                            self.err_no_input(diags);
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        };

                        let delim_tok = delim_tinfo.tok;
                        if delim_tok == LexToken::Comma {
                            self.tok_num += 1; // consume ','
                        } else if delim_tok == LexToken::CloseParen {
                            self.tok_num += 1; // consume ')'
                            break;
                        } else {
                            diags.err1(
                                "AST_38",
                                "Expected ',' or ')' in function call",
                                delim_tinfo.span(),
                            );
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        }
                    }
                }
            }

            // Identifiers are usually scalar variables or section names.
            // However, if an identifier is immediately followed by an open parenthesis `(`,
            // the parser actively eats tokens looking for the trailing close parenthesis `)`
            // to construct a generic function invocation, e.g., `foo(arg1, arg2)`. At this
            // AST stage, we parse all arguments without verifying function support. That
            // validation happens in later phases.
            LexToken::Identifier => {
                let id_nid = self.arena.new_node(self.tok_num);
                *top = Some(id_nid);
                self.tok_num += 1;

                // If an open parenthesis follows, we parse this as a function invocation.
                if let Some(next_tinfo) = self.peek()
                    && next_tinfo.tok == LexToken::OpenParen
                {
                    self.tok_num += 1; // consume '('

                    loop {
                        let Some(check_tinfo) = self.peek() else {
                            self.err_no_input(diags);
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        };

                        // A trailing close parenthesis indicates the end of the argument list.
                        if check_tinfo.tok == LexToken::CloseParen {
                            self.tok_num += 1; // consume ')'
                            break;
                        }

                        // Recursively parse the next argument within the parenthesis.
                        let mut arg_opt = None;
                        if !self.parse_pratt(0, &mut arg_opt, diags) {
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        }
                        if let Some(arg_nid) = arg_opt {
                            id_nid.append(arg_nid, &mut self.arena);
                        }

                        // Arguments must be separated by commas or terminated by a close parenthesis.
                        let Some(delim_tinfo) = self.peek() else {
                            self.err_no_input(diags);
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        };

                        let delim_tok = delim_tinfo.tok;
                        if delim_tok == LexToken::Comma {
                            self.tok_num += 1; // consume ','
                        } else if delim_tok == LexToken::CloseParen {
                            self.tok_num += 1; // consume ')'
                            break;
                        } else {
                            diags.err1(
                                "AST_38",
                                "Expected ',' or ')' in function call",
                                delim_tinfo.span(),
                            );
                            return self.dbg_exit_pratt("parse_pratt", &None, false);
                        }
                    }
                }
            }

            // Built-in functions with an optional identifier inside parens
            // ( [optional identifier] )
            LexToken::Addr | LexToken::AddrOffset | LexToken::SecOffset | LexToken::FileOffset => {
                // Create the node for the function and move past
                *top = Some(self.arena.new_node(self.tok_num));
                self.tok_num += 1;

                if !self.expect_token_no_add(LexToken::OpenParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
                if !self.optional_token(&[LexToken::Identifier], diags, top.unwrap()) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
                if !self.expect_token_no_add(LexToken::CloseParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
            }

            // Build-in functions with a mandatory identifier inside parens
            // ( <identifier> )
            LexToken::Sizeof => {
                *top = Some(self.arena.new_node(self.tok_num));
                self.tok_num += 1;

                if !self.expect_token_no_add(LexToken::OpenParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
                let mut arg_opt = None;
                if !self.parse_pratt(0, &mut arg_opt, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }

                // Check the children to determine if this sizeof() is valid.
                let is_valid = if let Some(arg_nid) = arg_opt {
                    let arg_tinfo = self.get_tinfo(arg_nid);
                    if arg_tinfo.tok == LexToken::Identifier {
                        // An identifier has children if parsed as a function call,
                        // which is an error.
                        !self.has_children(arg_nid)
                    } else if arg_tinfo.tok == LexToken::Namespace {
                        // A namespace has exactly one child (the trailing identifier) if called
                        // correctly as just an extension name, e.g. `sizeof(foo::bar)`.
                        // If the namespace has > 1 child, the user tried to pass arguments
                        // to sizeof(), which is an error.
                        arg_nid.children(&self.arena).count() == 1
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !is_valid {
                    let err_span = arg_opt.map_or(self.get_tinfo(top.unwrap()).span(), |nid| {
                        self.get_tinfo(nid).span()
                    });
                    diags.err1("AST_40", "sizeof() accepts only a section name or an extension identifier without arguments", err_span);
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }

                top.unwrap().append(arg_opt.unwrap(), &mut self.arena);
                if !self.expect_token_no_add(LexToken::CloseParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
            }

            // Built-in functions with a non-optional expression inside parens
            // ( <expr> )
            LexToken::ToI64 | LexToken::ToU64 => {
                *top = Some(self.arena.new_node(self.tok_num));
                self.tok_num += 1;

                if !self.expect_token_no_add(LexToken::OpenParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
                if !self.expect_expr(top.unwrap(), diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
                if !self.expect_token_no_add(LexToken::CloseParen, diags) {
                    return self.dbg_exit_pratt("parse_pratt", &None, false);
                }
            }

            // Built-in variable atoms — no parentheses, no arguments.
            LexToken::OutputSize | LexToken::OutputAddr => {
                *top = Some(self.arena.new_node(self.tok_num));
                self.tok_num += 1;
            }

            _ => {
                let msg = format!("Invalid expression operand '{}'", lhs_tinfo.val);
                diags.err1("AST_19", &msg, lhs_tinfo.span());
                return self.dbg_exit_pratt("parse_pratt", &None, false);
            }
        };

        // Clean exit if this expression had no more tokens
        if (*top).is_none() {
            return self.dbg_exit_pratt("parse_pratt", &None, true);
        }

        // Keep processing for the remaining right hand side of the expression.
        loop {
            // We expect an operation such as add, a semicolon, etc. or the end of input.
            let op_tinfo = self.peek();
            if op_tinfo.is_none() {
                break; // end of input.
            }

            // Filter disallowed operations.
            let op_tinfo = op_tinfo.unwrap();
            let tok = op_tinfo.tok;

            // Comma, close paren and semicolon are terminating conditions
            // because some upper layer is specifically looking for them.
            if matches!(
                tok,
                LexToken::Comma | LexToken::CloseParen | LexToken::Semicolon
            ) {
                break;
            }

            let Some((lbp, rbp)) = Ast::get_infix_binding_power(tok) else {
                let msg = format!("Invalid operation '{}'", op_tinfo.val);
                diags.err1("AST_9", &msg, op_tinfo.span());
                return self.dbg_exit_pratt("parse_pratt", &None, false);
            };

            debug!(
                "Ast::parse_pratt: operation '{}' with (lbp,rbp) = ({},{})",
                op_tinfo.val, lbp, rbp
            );

            // A decrease in operator precedence ends the iteration.
            if lbp < min_bp {
                break;
            }

            let op_nid = self.arena.new_node(self.tok_num);
            self.tok_num += 1;

            // Attach the old top as a child of the operation,
            // then update the new top node
            op_nid.append(top.unwrap(), &mut self.arena);

            // The operation is the new left-hand-side from our caller's point of view
            *top = Some(op_nid);

            // Recurse into the right hand side of the operation, if any
            let mut rhs_opt = None;
            if !self.parse_pratt(rbp, &mut rhs_opt, diags) {
                return self.dbg_exit_pratt("parse_pratt", &None, false);
            }

            if let Some(rhs_nid) = rhs_opt {
                op_nid.append(rhs_nid, &mut self.arena);
            } else {
                // RHS is none
                break;
            }
        }

        self.dbg_exit_pratt("parse_pratt", top, true)
    }

    /// Parses a keyword statement whose operands are one or more comma-separated
    /// expressions.  Handles `wr8`..`wr64`, `wrs`, `wrf`, `assert`, `align`,
    /// `set*`, and `print`.
    ///
    /// ```text
    /// print <expr> [, <expr>] ;
    ///
    ///   <keyword>        <- root node (e.g. print, wr8, assert)
    ///   ├── <expr>       <- first expression (sub-tree from parse_pratt)
    ///   └── [<expr>...]  <- additional comma-separated expressions, if any
    /// ```
    fn parse_expr(&mut self, parent: NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_multi_expr");
        let mut result = true;
        // Add the print keyword as a child of the parent
        let print_nid = self.add_to_parent_and_advance(parent);

        let mut expr_opt = None;

        // Loop until we run out of comma separated expressions.
        // After each return from parse_pratt, we should be pointing at a comma.
        loop {
            result &= self.parse_pratt(0, &mut expr_opt, diags);
            if !result {
                break; // error occurred
            }
            if let Some(expr_nid) = expr_opt {
                print_nid.append(expr_nid, &mut self.arena);

                // Omit the comma from the AST to reduce clutter.
                if let Some(tinfo) = self.peek()
                    && tinfo.tok == LexToken::Comma
                {
                    self.tok_num += 1;
                    continue;
                }

                // If not a comma, then we expect semi.
                result &= self.expect_semi(diags, print_nid);
                break;
            } else {
                // the print statement ended in some unusual way, e.g. trailing comma.
                // fuzz test found this with print 1,;
                let msg = "Statement ended unexpectedly";
                let tinfo = self.get_tinfo(print_nid);
                diags.err1("AST_21", msg, tinfo.span());
                result = false;
                break;
            }
        }

        self.dbg_exit("parse_multi_expr", result)
    }

    /// Parses a `label` statement.  Labels mark a named address within a section
    /// body and produce no output bytes; they exist solely so other expressions
    /// can reference the address via `addr()`, `addr_offset()`, or `sec_offset()`.
    ///
    /// ```text
    /// label <name>;
    ///
    ///   label            <- leaf node, value holds the label name
    /// ```
    fn parse_label(&mut self, parent: NodeId, _diags: &mut Diags) -> bool {
        // Not much to do since labels just mark a place but
        // cause no actions.
        self.dbg_enter("parse_label");
        self.add_to_parent_and_advance(parent);
        self.dbg_exit("parse_assert", true)
    }

    /// Parses the top-level `output` statement that designates which section
    /// is written to the output file and an optional absolute base address.
    ///
    /// ```text
    /// output <name> [<addr>];
    ///
    ///   output               <- root node for the output declaration
    ///   ├── <Identifier>     <- name of the section to emit
    ///   └── [<U64|Integer>]  <- optional absolute start address
    /// ```
    fn parse_output(&mut self, parent: NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_output");
        let mut result = false;
        // Add the section keyword as a child of the parent and advance
        let output_nid = self.add_to_parent_and_advance(parent);

        // After 'output' a section identifier is expected
        if self.expect_leaf(
            diags,
            output_nid,
            LexToken::Identifier,
            "AST_7",
            "Expected a section name after output",
        ) {
            // After the section identifier, an optional absolute starting address
            // (which may be a literal or a const identifier)
            result = self.optional_token(
                &[LexToken::U64, LexToken::Integer, LexToken::Identifier],
                diags,
                output_nid,
            );

            // finally a semicolon
            result &= self.expect_semi(diags, output_nid);
        }

        self.dbg_exit("parse_output", result)
    }

    /// Parses a `const` declaration and attaches it to the AST.
    ///
    /// ```text
    /// const <name> = <expr>;
    ///
    ///   const            <- root node for a const declaration
    ///   ├── <Identifier> <- constant name
    ///   ├── =            <- syntactic separator, not an operation
    ///   └── <expr>       <- right-hand side (literal or expression)
    /// ```
    ///
    /// The `=` node is retained as a child to preserve source location
    /// information but carries no semantic meaning in later pipeline stages.
    fn parse_const(&mut self, parent: NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_const");
        let mut result = false;
        // Add the const keyword as a child of the parent and advance
        let const_nid = self.add_to_parent_and_advance(parent);

        // After 'const' an identifier is expected
        if self.expect_leaf(
            diags,
            const_nid,
            LexToken::Identifier,
            "AST_8",
            "Expected an identifier after 'const'",
        ) {
            // After the identifier, an equals sign is expected
            if self.expect_token(LexToken::Eq, diags, const_nid) {
                // After the equals sign, a literal or literal expression is expected.
                // The literal expression can be literals combined with other consts.
                if self.expect_expr(const_nid, diags) {
                    result = self.expect_semi(diags, const_nid);
                }
            }
        }
        self.dbg_exit("parse_const", result)
    }

    /// Adds the current token as a child of the parent and advances
    /// the token index.  The current token MUST BE VALID!
    fn parse_leaf(&mut self, parent: NodeId) {
        let nid = self.arena.new_node(self.tok_num);
        parent.append(nid, &mut self.arena);
        self.tok_num += 1;
    }

    pub fn get_tinfo(&self, nid: NodeId) -> &'toks TokenInfo<'_> {
        let tok_num = *self.arena[nid].get();
        &self.tv[tok_num]
    }

    const DOT_DEFAULT_FILL: &'static str = "#F2F2F2";
    const DOT_DEFAULT_EDGE: &'static str = "#808080";
    const DOT_DEFAULT_PEN: &'static str = "#808080";

    fn dump_r(&self, nid: NodeId, depth: usize, file: &mut File) -> anyhow::Result<()> {
        debug!(
            "AST: {}: {}{}",
            nid,
            " ".repeat(depth * 4),
            self.get_tinfo(nid).val
        );
        let tinfo = self.get_tinfo(nid);

        let (label, color) = match tinfo.tok {
            LexToken::QuotedString => {
                if tinfo.val.len() <= 8 {
                    (
                        tinfo
                            .val
                            .strip_prefix('\"')
                            .unwrap()
                            .strip_suffix('\"')
                            .unwrap(),
                        Ast::DOT_DEFAULT_FILL,
                    )
                } else {
                    ("<string>", Ast::DOT_DEFAULT_FILL)
                }
            }
            LexToken::Unknown => ("<unknown>", "red"),
            _ => (tinfo.val, Ast::DOT_DEFAULT_FILL),
        };

        file.write(format!("{} [label=\"{}\",fillcolor=\"{}\"]\n", nid, label, color).as_bytes())
            .context("ast.dot write failed")?;
        let children = nid.children(&self.arena);
        for child_nid in children {
            /*
            let child_tinfo = self.get_tinfo(child_nid);
            if child_tinfo.tok == LexToken::Semicolon {
                continue;
            }
            */

            file.write(format!("{} -> {}\n", nid, child_nid).as_bytes())
                .context("ast.dot write failed")?;
            self.dump_r(child_nid, depth + 1, file)?;
        }
        Ok(())
    }

    /**
     * Recursively dumps the AST to the console.
     */
    pub fn dump(&self, fname: &str) -> anyhow::Result<()> {
        debug!("");

        let mut file = File::create(fname)
            .context(format!("Error attempting to create debug file '{}'", fname))?;
        file.write(b"digraph {\n").context("ast.dot write failed")?;
        file.write(
            format!(
                "node [style=filled,fillcolor=\"{}\",color=\"{}\"]\n",
                Ast::DOT_DEFAULT_FILL,
                Ast::DOT_DEFAULT_PEN
            )
            .as_bytes(),
        )
        .context("ast.dot write failed")?;
        file.write(format!("edge [color=\"{}\"]\n", Ast::DOT_DEFAULT_EDGE).as_bytes())
            .context("ast.dot write failed")?;

        file.write(format!("{} [label=\"root\"]\n", self.root).as_bytes())
            .context("ast.dot write failed")?;
        let children = self.root.children(&self.arena);
        for child_nid in children {
            file.write(format!("{} -> {}\n", self.root, child_nid).as_bytes())
                .context("ast.dot write failed")?;
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
        Section {
            tinfo: ast.get_tinfo(nid),
            nid,
        }
    }
}

/*******************************
 * Const
 ******************************/
#[derive(Debug)]
pub struct Const<'toks> {
    pub tinfo: &'toks TokenInfo<'toks>,
    pub nid: NodeId,
}

impl<'toks> Const<'toks> {
    pub fn new(ast: &'toks Ast, nid: NodeId) -> Const<'toks> {
        Const {
            tinfo: ast.get_tinfo(nid),
            nid,
        }
    }
}

/*******************************
 * Label
 ******************************/
#[derive(Debug)]
pub struct Label {
    pub nid: NodeId,

    /// Location in source code of the label
    pub loc: SourceSpan,
}

/*******************************
 * Output
 ******************************/
#[derive(Clone, Debug)]
pub struct Output<'toks> {
    pub tinfo: &'toks TokenInfo<'toks>,
    pub nid: NodeId,
    pub sec_nid: NodeId,
    pub addr_nid: Option<NodeId>,
}

impl<'toks> Output<'toks> {
    /// Create an new output object
    pub fn new(ast: &'toks Ast, nid: NodeId) -> Output<'toks> {
        let mut children = nid.children(&ast.arena);
        // the section name is the first child of the output
        // AST processing guarantees this exists.
        let sec_nid = children.next().unwrap();

        // Optional start address is the second child.
        let addr_nid = children.next();
        Output {
            tinfo: ast.get_tinfo(nid),
            nid,
            sec_nid,
            addr_nid,
        }
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
    pub labels: HashMap<&'toks str, Label>,
    pub consts: HashMap<&'toks str, Const<'toks>>,
    pub output: Output<'toks>,
    pub global_asserts: Vec<NodeId>,
    //pub properties: HashMap<NodeId, NodeProperty>
}

impl<'toks> AstDb<'toks> {
    // Control recursion to some safe level.  100 is just a guesstimate.
    const MAX_RECURSION_DEPTH: usize = 100;

    /// Processes a section in the AST
    /// All section names are also label names
    fn record_section(
        diags: &mut Diags,
        sec_nid: NodeId,
        ast: &'toks Ast,
        sections: &mut HashMap<&'toks str, Section<'toks>>,
    ) -> bool {
        debug!("AstDb::record_section: NodeId {}", sec_nid);

        let mut children = sec_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        if is_reserved_identifier(sec_str) {
            let m = format!(
                "'{}' is a reserved identifier and cannot be used as a section name",
                sec_str
            );
            diags.err1("AST_32", &m, sec_tinfo.span());
            return false;
        }
        if sections.contains_key(sec_str) {
            // error, duplicate section names
            // We know the section exists, so unwrap is fine.
            let orig_section = sections.get(sec_str).unwrap();
            let orig_tinfo = orig_section.tinfo;
            let m = format!("Duplicate section name '{}'", sec_str);
            diags.err2("AST_29", &m, sec_tinfo.span(), orig_tinfo.span());
            return false;
        }
        sections.insert(sec_str, Section::new(ast, sec_nid));
        true
    }

    /// Processes a const in the AST
    fn record_const(
        diags: &mut Diags,
        sec_nid: NodeId,
        ast: &'toks Ast,
        consts: &mut HashMap<&'toks str, Const<'toks>>,
    ) -> bool {
        debug!("AstDb::record_const: NodeId {}", sec_nid);

        let mut children = sec_nid.children(&ast.arena);
        let sec_name_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        if is_reserved_identifier(sec_str) {
            let m = format!(
                "'{}' is a reserved identifier and cannot be used as a const name",
                sec_str
            );
            diags.err1("AST_33", &m, sec_tinfo.span());
            return false;
        }
        if consts.contains_key(sec_str) {
            // error, duplicate const names
            // We know the const exists, so unwrap is fine.
            let orig_const = consts.get(sec_str).unwrap();
            let orig_tinfo = orig_const.tinfo;
            let m = format!("Duplicate const name '{}'", sec_str);
            diags.err2("AST_30", &m, sec_tinfo.span(), orig_tinfo.span());
            return false;
        }
        consts.insert(sec_str, Const::new(ast, sec_nid));
        true
    }

    /// Returns true if the specified child of the specified node is a section
    /// name that exists.  Otherwise, prints a diagnostic and returns false.
    fn validate_section_name(
        &self,
        child_num: usize,
        parent_nid: NodeId,
        ast: &'toks Ast,
        diags: &mut Diags,
    ) -> bool {
        debug!(
            "AstDb::validate_section_name: NodeId {} for child {}",
            parent_nid, child_num
        );

        let mut children = parent_nid.children(&ast.arena);

        // First, advance to the specified child number
        let mut num = 0;
        while num < child_num {
            let sec_name_nid_opt = children.next();
            if sec_name_nid_opt.is_none() {
                // error, not enough children to reach section name
                let m = "Missing section name".to_string();
                let section_tinfo = ast.get_tinfo(parent_nid);
                diags.err1("AST_23", &m, section_tinfo.span());
                return false;
            }
            num += 1;
        }
        let sec_name_nid_opt = children.next();
        if sec_name_nid_opt.is_none() {
            // error, specified section does not exist
            let m = "Missing section name".to_string();
            let section_tinfo = ast.get_tinfo(parent_nid);
            diags.err1("AST_11", &m, section_tinfo.span());
            return false;
        }
        let sec_name_nid = sec_name_nid_opt.unwrap();
        let sec_tinfo = ast.get_tinfo(sec_name_nid);
        let sec_str = sec_tinfo.val;
        if !self.sections.contains_key(sec_str) {
            // error, specified section does not exist
            let m = format!("Unknown or unreachable section name '{}'", sec_str);
            diags.err1("AST_16", &m, sec_tinfo.span());
            return false;
        }
        true
    }

    pub fn record_output(
        diags: &mut Diags,
        nid: NodeId,
        ast: &'toks Ast,
        output: &mut Option<Output<'toks>>,
    ) -> bool {
        let tinfo = ast.get_tinfo(nid);
        if output.is_some() {
            let m = "Multiple output statements are not allowed.";
            let orig_tinfo = output.as_ref().unwrap().tinfo;
            diags.err2("AST_10", m, orig_tinfo.span(), tinfo.span());
            return false;
        }

        *output = Some(Output::new(ast, nid));
        true // succeed
    }

    /// Recursively validate the basic hierarchy of the AST object.
    /// Nested sections tracks the current hierarchy of section writes so we
    /// catch cycles.
    fn validate_nesting_r(
        &mut self,
        rdepth: usize,
        parent_nid: NodeId,
        ast: &'toks Ast,
        nested_sections: &mut HashSet<&'toks str>,
        diags: &mut Diags,
    ) -> bool {
        debug!(
            "AstDb::validate_nesting_r: ENTER at depth {} for parent nid: {}",
            rdepth, parent_nid
        );

        if rdepth > AstDb::MAX_RECURSION_DEPTH {
            let tinfo = ast.get_tinfo(parent_nid);
            let m = format!(
                "Maximum recursion depth ({}) exceeded when processing '{}'.",
                AstDb::MAX_RECURSION_DEPTH,
                tinfo.val
            );
            diags.err1("AST_5", &m, tinfo.span());
            return false;
        }

        let mut result = true;
        let tinfo = ast.get_tinfo(parent_nid);
        result &= match tinfo.tok {
            // Wr statement must specify a valid section name
            LexToken::Wr => {
                let mut children = parent_nid.children(&ast.arena);
                // the section name is the first child of the output
                // AST processing guarantees this exists.
                let sec_nid = children.next().unwrap();
                let sec_tinfo = ast.get_tinfo(sec_nid);

                if sec_tinfo.tok == LexToken::Identifier && !ast.has_children(sec_nid) {
                    if !self.validate_section_name(0, parent_nid, ast, diags) {
                        return false;
                    }

                    let sec_str = sec_tinfo.val;

                    // Make sure we haven't already recursed through this section.
                    if nested_sections.contains(sec_str) {
                        let m = "Writing section creates a cycle.";
                        diags.err1("AST_6", m, sec_tinfo.span());
                        false
                    } else {
                        // add this section to our nested sections tracker
                        nested_sections.insert(sec_str);
                        let section = self.sections.get(sec_str).unwrap();
                        let children = section.nid.children(&ast.arena);
                        for nid in children {
                            result &= self.validate_nesting_r(
                                rdepth + 1,
                                nid,
                                ast,
                                nested_sections,
                                diags,
                            );
                        }
                        // We're done with the section, so remove it from the nesting hash.
                        nested_sections.remove(sec_str);
                        result
                    }
                } else {
                    true
                }
            }
            _ => {
                // When no children exist, this case terminates recursion.
                let children = parent_nid.children(&ast.arena);
                for nid in children {
                    result &= self.validate_nesting_r(rdepth + 1, nid, ast, nested_sections, diags);
                }
                result
            }
        };

        debug!(
            "AstDb::validate_nesting_r: EXIT({}) at depth {} for nid: {}",
            result, rdepth, parent_nid
        );
        result
    }

    pub fn new(diags: &mut Diags, ast: &'toks Ast) -> anyhow::Result<AstDb<'toks>> {
        debug!("AstDb::new");

        // Populate the AST database of critical structures.
        let mut result = true;

        let mut sections: HashMap<&'toks str, Section<'toks>> = HashMap::new();
        let mut output: Option<Output<'toks>> = None;
        let mut consts: HashMap<&'toks str, Const<'toks>> = HashMap::new();
        let mut global_asserts: Vec<NodeId> = Vec::new();

        // First phase, record all sections, files, and the output.
        // These are defined only at top level so no need for recursion.
        for nid in ast.root.children(&ast.arena) {
            let tinfo = ast.get_tinfo(nid);
            result = result
                && match tinfo.tok {
                    LexToken::Section => Self::record_section(diags, nid, ast, &mut sections),
                    LexToken::Output => Self::record_output(diags, nid, ast, &mut output),
                    LexToken::Const => Self::record_const(diags, nid, ast, &mut consts),
                    // Global asserts are collected here and linearized later.
                    // They have no name to record in any map.
                    LexToken::Assert => { global_asserts.push(nid); true }
                    _ => {
                        let msg = format!("Invalid top-level expression {}", tinfo.val);
                        diags.err1("AST_24", &msg, tinfo.span().clone());
                        diags.note0(
                            "AST_25",
                            "At top-level, allowed expressions are 'section' and 'output'",
                        );
                        false
                    }
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

        // Check for const names that conflict with section names.
        for (name, const_item) in &consts {
            if let Some(sec_item) = sections.get(name) {
                let m = format!("Const name '{}' conflicts with a section name", name);
                diags.err2("AST_31", &m, const_item.tinfo.span(), sec_item.tinfo.span());
                result = false;
            }
        }

        if !result {
            bail!("AST construction failed");
        }

        let output_nid = output.as_ref().unwrap().nid;
        let mut ast_db = AstDb {
            sections,
            labels: HashMap::new(),
            consts,
            output: output.unwrap(),
            global_asserts,
        };

        if !ast_db.validate_section_name(0, output_nid, ast, diags) {
            bail!("AST construction failed");
        }

        let mut children = output_nid.children(&ast.arena);
        // the section name is the first child of the output
        // AST processing guarantees this exists.
        let sec_nid = children.next().unwrap();
        let sec_tinfo = ast.get_tinfo(sec_nid);
        let sec_str = sec_tinfo.val;

        // add the output section to our nested sections tracker
        let mut nested_sections = HashSet::new();
        nested_sections.insert(sec_str);
        let section = ast_db.sections.get(sec_str).unwrap();

        // We're going to need this iterator more than once
        let children = section.nid.children(&ast.arena);

        for nid in children {
            result &= ast_db.validate_nesting_r(1, nid, ast, &mut nested_sections, diags);
        }

        if !result {
            bail!("AST construction failed");
        }

        Ok(ast_db)
    }
}
