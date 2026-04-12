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

mod lexer;
use lexer::Lexer;

use anyhow::{Context, bail};
use diags::{Diags, SourceSpan};
use indextree::{Arena, NodeId};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::prelude::*;

#[allow(unused_imports)]
use tracing::{debug, error, info, trace, warn};

/// All tokens in brink.
/// Keep this simple and do not be tempted to attach
/// unstructured values to these enum variants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LexToken {
    Const,
    If,
    Else,
    BuiltinOutputSize,
    BuiltinOutputAddr,
    BuiltinVersionString,
    BuiltinVersionMajor,
    BuiltinVersionMinor,
    BuiltinVersionPatch,
    Section,
    Align,
    SetSecOffset,
    SetAddrOffset,
    SetAddr,
    SetFileOffset,
    Assert,
    Sizeof,
    Print,
    ToU64,
    ToI64,
    Addr,
    AddrOffset,
    SecOffset,
    FileOffset,
    Wrs,
    Wr8,
    Wr16,
    Wr24,
    Wr32,
    Wr40,
    Wr48,
    Wr56,
    Wr64,
    Wrf,
    Wr,
    Output,
    DoubleEq,
    NEq,
    GEq,
    LEq,
    Gt,
    Lt,
    Eq,
    DoubleAmpersand,
    DoublePipe,
    Ampersand,
    Pipe,
    Plus,
    Minus,
    Asterisk,
    FSlash,
    Percent,
    Comma,
    DoubleLess,
    DoubleGreater,
    OpenBrace,
    CloseBrace,
    OpenParen,
    CloseParen,
    Semicolon,
    Label,
    Namespace,
    Identifier,
    Integer,
    U64,
    I64,
    QuotedString,
    /// Catch-all for unrecognized input.
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
    if let Some(rest) = name.strip_prefix("wr")
        && rest.starts_with(|c: char| c.is_ascii_digit())
    {
        return true;
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
#[derive(Clone)]
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

/// Context passed through parse_if / parse_if_body to control which
/// statements are legal inside an if/else body.
#[derive(Clone, Copy)]
enum ParseIfContext {
    /// if/else at the top level or nested inside another const if/else.
    /// Only const-compatible statements are allowed.
    TopLevel,
    /// if/else directly inside a section body.
    /// All section-level statements are allowed in addition to const ones.
    Section,
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
        let mut lex = Lexer::new(fstr);
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
                LexToken::If => self.parse_if_r(self.root, diags, ParseIfContext::TopLevel),
                LexToken::Identifier => {
                    let ok = self.parse_deferred_assign(self.root, diags);
                    if !ok {
                        // If parsing of an identifier fails, consume the whole statement
                        // to avoid sending bad tokens to later phases.  Reason being
                        // an unknown identifier can be all sort of arbitrary chars and
                        // is hard to make sense of later on.
                        self.advance_past_semicolon();
                    }
                    ok
                }
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

    /// Returns `true` if `tok` is one of the tokens that a section body dispatches
    /// to `parse_expr`.  Centralized here so adding a new write instruction (e.g.
    /// a future `Wr128`) only requires updating this one predicate.
    fn is_section_expr_tok(tok: LexToken) -> bool {
        matches!(
            tok,
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
                | LexToken::Print
        )
    }

    /// Try to parse one section-body statement starting at the current token.
    ///
    /// Handles the three dispatch cases shared by `parse_section_contents` and
    /// `parse_if_body_r` (Section context):
    ///   - `Label`                        → `parse_label`
    ///   - `Wr`                           → `parse_wr`
    ///   - `is_section_expr_tok` tokens   → `parse_expr`
    ///
    /// Returns `Some(result)` if the token was handled, `None` if it is not a
    /// recognized section-level token (caller is responsible for the error).
    fn try_parse_section_stmt(&mut self, parent: NodeId, diags: &mut Diags) -> Option<bool> {
        let tok = self.peek()?.tok;
        match tok {
            LexToken::Label => Some(self.parse_label(parent, diags)),
            LexToken::Wr => Some(self.parse_wr(parent, diags)),
            tok if Self::is_section_expr_tok(tok) => Some(self.parse_expr(parent, diags)),
            _ => None,
        }
    }

    /// Parses the body of a section, appending statement nodes directly to the
    /// parent section node.  Loops until a `}` is found or tokens are exhausted.
    /// Each iteration dispatches via `try_parse_section_stmt`; unrecognized tokens
    /// produce a diagnostic and are skipped to the next `;` to allow recovery.
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
            let parse_ok = if tinfo.tok == LexToken::If {
                self.parse_if_r(parent, diags, ParseIfContext::Section)
            } else if let Some(result) = self.try_parse_section_stmt(parent, diags) {
                result
            } else {
                self.err_invalid_expression(diags, "AST_3");
                false
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
            LexToken::DoubleEq
            | LexToken::NEq
            | LexToken::LEq
            | LexToken::GEq
            | LexToken::Lt
            | LexToken::Gt => Some((9, 10)),
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
            LexToken::BuiltinOutputSize
            | LexToken::BuiltinOutputAddr
            | LexToken::BuiltinVersionString
            | LexToken::BuiltinVersionMajor
            | LexToken::BuiltinVersionMinor
            | LexToken::BuiltinVersionPatch => {
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

            // Comma, close paren, semicolon, open brace, and else are terminating
            // conditions because some upper layer is specifically looking for them.
            if matches!(
                tok,
                LexToken::Comma
                    | LexToken::CloseParen
                    | LexToken::Semicolon
                    | LexToken::OpenBrace
                    | LexToken::Else
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
    /// const <name>;  // deferred assignment
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
            // After the identifier: either '=' (full definition) or ';' (declare-only).
            if let Some(tinfo) = self.peek() {
                if tinfo.tok == LexToken::Eq {
                    // Full definition: const NAME = expr;
                    if self.expect_token(LexToken::Eq, diags, const_nid)
                        && self.expect_expr(const_nid, diags)
                    {
                        result = self.expect_semi(diags, const_nid);
                    }
                } else if tinfo.tok == LexToken::Semicolon {
                    // Declare-only: const NAME;  (value assigned later in an if/else body)
                    result = self.expect_semi(diags, const_nid);
                } else {
                    self.err_expected_after(
                        diags,
                        "AST_50",
                        "Expected '=' or ';' after const identifier",
                    );
                }
            } else {
                self.err_no_input(diags);
            }
        }
        self.dbg_exit("parse_const", result)
    }

    /// Recursively parses an `if/else` statement and attaches it to the AST.
    ///
    /// ```text
    /// if <expr> { <body> } [else { <body> } | else if ...]
    ///
    ///   if                  <- root node
    ///   ├── <condition>     <- pratt expression
    ///   ├── {
    ///   ├── [then_stmts...] <- see parse_if_body
    ///   ├── }
    ///   [├── else
    ///    ├── {              <- or nested if node for `else if`
    ///    ├── [else_stmts...]
    ///    └── }]
    /// ```
    fn parse_if_r(&mut self, parent: NodeId, diags: &mut Diags, ctx: ParseIfContext) -> bool {
        self.dbg_enter("parse_if");
        // Consume 'if' and create root node
        let if_nid = self.add_to_parent_and_advance(parent);

        // Parse condition expression
        if !self.expect_expr(if_nid, diags) {
            return self.dbg_exit("parse_if", false);
        }

        // Expect opening brace for then-body
        let brace_toknum = self.tok_num;
        if !self.expect_leaf(
            diags,
            if_nid,
            LexToken::OpenBrace,
            "AST_51",
            "Expected '{' after if condition",
        ) {
            return self.dbg_exit("parse_if", false);
        }
        if !self.parse_if_body_r(if_nid, diags, brace_toknum, ctx) {
            return self.dbg_exit("parse_if", false);
        }

        // Check for optional else clause
        let result = if let Some(tinfo) = self.peek() {
            if tinfo.tok == LexToken::Else {
                self.add_to_parent_and_advance(if_nid); // consume 'else', add as child
                if let Some(next) = self.peek() {
                    if next.tok == LexToken::If {
                        // else if: parse nested if directly (no brace wrapper)
                        self.parse_if_r(if_nid, diags, ctx)
                    } else if next.tok == LexToken::OpenBrace {
                        let else_brace = self.tok_num;
                        self.add_to_parent_and_advance(if_nid); // consume '{'
                        self.parse_if_body_r(if_nid, diags, else_brace, ctx)
                    } else {
                        self.err_expected_after(
                            diags,
                            "AST_52",
                            "Expected '{' or 'if' after 'else'",
                        );
                        false
                    }
                } else {
                    self.err_no_input(diags);
                    false
                }
            } else {
                true // no else clause
            }
        } else {
            self.err_no_input(diags);
            false
        };

        self.dbg_exit("parse_if", result)
    }

    /// Recursively (by way of parse_if_r) parses the body of an if/else statement.
    ///
    /// In `TopLevel` context, only const-compatible statements are allowed:
    /// bare assignment (`IDENT = expr;`), `print`, `assert`, and nested `if/else`.
    ///
    /// In `Section` context, all section-level statements are also allowed:
    /// `wr`, `wr8`–`wr64`, `wrs`, `wrf`, `align`, `set_*`, `label:`, and nested `if/else`.
    fn parse_if_body_r(
        &mut self,
        parent: NodeId,
        diags: &mut Diags,
        brace_tok_num: usize,
        ctx: ParseIfContext,
    ) -> bool {
        self.dbg_enter("parse_if_body");
        let mut result = true;
        let mut tok_num_old = 0;

        while let Some(tinfo) = self.peek() {
            if tok_num_old == self.tok_num {
                self.tok_num += 1;
                continue;
            }
            tok_num_old = self.tok_num;

            if tinfo.tok == LexToken::CloseBrace {
                self.parse_leaf(parent); // add '}' as child
                return self.dbg_exit("parse_if_body", result);
            }

            let parse_ok = match tinfo.tok {
                // Const-compatible statements (allowed in both TopLevel and Section)
                LexToken::Identifier => self.parse_deferred_assign(parent, diags),
                LexToken::Print | LexToken::Assert => self.parse_expr(parent, diags),
                LexToken::If => self.parse_if_r(parent, diags, ctx),
                // Section-level statements: delegate to the shared dispatcher.
                // try_parse_section_stmt returns None for unrecognized tokens.
                _ => {
                    // Snapshot before mutable borrows below.
                    let err_val = tinfo.val.to_string();
                    let err_span = tinfo.span();
                    let tok = tinfo.tok;
                    let maybe = match ctx {
                        ParseIfContext::Section => self.try_parse_section_stmt(parent, diags),
                        ParseIfContext::TopLevel if tok == LexToken::Section => {
                            Some(self.parse_section(parent, diags))
                        }
                        _ => None,
                    };
                    maybe.unwrap_or_else(|| {
                        let msg = format!("'{}' is not allowed inside an if/else body", err_val);
                        diags.err1("AST_53", &msg, err_span);
                        false
                    })
                }
            };

            if !parse_ok {
                self.advance_past_semicolon();
                result = false;
            }
        }

        self.err_no_close_brace(diags, brace_tok_num);
        self.dbg_exit("parse_if_body", false)
    }

    /// Parses a deferred assignment statement `IDENT = expr ;` inside an if/else body.
    ///
    /// ```text
    ///   =                  <- root node (the assignment operator)
    ///   ├── <Identifier>   <- LHS name
    ///   └── <expr>         <- RHS value expression
    /// ```
    fn parse_deferred_assign(&mut self, parent: NodeId, diags: &mut Diags) -> bool {
        self.dbg_enter("parse_deferred_assign");

        // Confirm the token after the identifier is '=' (not '==', which would be a comparison).
        let next_tok = self.tv.get(self.tok_num + 1).map(|t| t.tok);
        if next_tok != Some(LexToken::Eq) {
            let msg = format!(
                "Expected '=' after identifier in deferred const assignment, found '{}'",
                self.tv
                    .get(self.tok_num + 1)
                    .map(|t| t.val)
                    .unwrap_or("<end of input>")
            );
            diags.err1("AST_54", &msg, self.tv[self.tok_num].span());
            // We don't want the arbitrary chars in an unknown identifier to
            // propogate any further, so eat the identifier here.
            self.tok_num += 1;
            return self.dbg_exit("parse_deferred_assign", false);
        }

        // Create the identifier node without attaching it to a parent yet.
        let ident_nid = self.arena.new_node(self.tok_num);
        self.tok_num += 1;

        // Create the Eq node as the statement root (attached to parent).
        let eq_nid = self.add_to_parent_and_advance(parent);

        // Attach identifier as the first child of the Eq node.
        eq_nid.append(ident_nid, &mut self.arena);

        // Parse RHS expression as the second child.
        if !self.expect_expr(eq_nid, diags) {
            return self.dbg_exit("parse_deferred_assign", false);
        }

        let result = self.expect_semi(diags, eq_nid);
        self.dbg_exit("parse_deferred_assign", result)
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

    /// Returns the root NodeId of the AST.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Returns a mutable reference to the underlying indextree arena.
    /// Callers can use any indextree `NodeId` operation that requires `&mut Arena`.
    pub fn arena_mut(&mut self) -> &mut Arena<usize> {
        &mut self.arena
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
    pub output: Output<'toks>,
    pub global_asserts: Vec<NodeId>,
    /// All top-level const definitions, const declarations, and if/else blocks
    /// in their original source token order.
    pub const_statements: Vec<NodeId>,
    /// Set of all const names for collision detection.
    pub const_names: HashMap<&'toks str, SourceSpan>,
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

    /// Records a const identifier in the AST.
    /// Checks for duplicate names and reserved words.
    fn record_const(
        diags: &mut Diags,
        const_nid: NodeId,
        ast: &'toks Ast,
        consts: &mut HashMap<&'toks str, SourceSpan>,
    ) -> bool {
        debug!("AstDb::record_const: NodeId {}", const_nid);

        let mut children = const_nid.children(&ast.arena);
        let const_name_nid = children.next().unwrap();
        let const_tinfo = ast.get_tinfo(const_name_nid);
        let const_str = const_tinfo.val;
        if is_reserved_identifier(const_str) {
            let m = format!(
                "'{}' is a reserved identifier and cannot be used as a const name",
                const_str
            );
            diags.err1("AST_33", &m, const_tinfo.span());
            return false;
        }
        if let Some(orig_span) = consts.get(const_str) {
            let m = format!("Duplicate const name '{}'", const_str);
            diags.err2("AST_30", &m, const_tinfo.span(), orig_span.clone());
            return false;
        }
        consts.insert(const_str, const_tinfo.span());
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

    /// Build an `AstDb` from `ast`.
    ///
    /// When `validate` is `true` (the normal post-prune call), the output section's
    /// full nesting tree is walked to catch circular references and unknown `wr`
    /// targets.  When `false` (the pre-prune call used only for `const_eval`), that
    /// walk is skipped so that `wr` references to sections defined inside top-level
    /// `if` blocks do not produce false-positive errors before those blocks are pruned.
    pub fn new(diags: &mut Diags, ast: &'toks Ast, validate: bool) -> anyhow::Result<AstDb<'toks>> {
        debug!("AstDb::new");

        // Populate the AST database of critical structures.
        let mut result = true;

        // All sections, mapping section name to AST node.
        let mut sections: HashMap<&'toks str, Section<'toks>> = HashMap::new();
        // The single required output statement.
        let mut output: Option<Output<'toks>> = None;
        // All top-level assert statements.
        let mut global_asserts: Vec<NodeId> = Vec::new();
        // All const declarations and conditional blocks in source code order.
        let mut const_statements: Vec<NodeId> = Vec::new();
        // All declared const names and their locations for duplicate detection.
        let mut const_names: HashMap<&'toks str, SourceSpan> = HashMap::new();

        // First phase, record all sections, files, and the output.
        // These are defined only at top level so no need for recursion.
        for nid in ast.root.children(&ast.arena) {
            let tinfo = ast.get_tinfo(nid);
            result = result
                && match tinfo.tok {
                    LexToken::Section => Self::record_section(diags, nid, ast, &mut sections),
                    LexToken::Output => Self::record_output(diags, nid, ast, &mut output),
                    LexToken::Const => {
                        const_statements.push(nid);
                        Self::record_const(diags, nid, ast, &mut const_names)
                    }
                    LexToken::If => {
                        // We evaluate if statements a const-time, so expect
                        // only const-compatible statements inside.
                        const_statements.push(nid);
                        true
                    }
                    LexToken::Eq => {
                        const_statements.push(nid);
                        true
                    }
                    // Global asserts are collected here and linearized later.
                    // They have no name to record in any map.
                    LexToken::Assert => {
                        global_asserts.push(nid);
                        true
                    }
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
        for (const_name, const_span) in &const_names {
            if let Some(sec_item) = sections.get(const_name) {
                let m = format!("Const name '{}' conflicts with a section name", const_name);
                diags.err2("AST_31", &m, const_span.clone(), sec_item.tinfo.span());
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
            output: output.unwrap(),
            global_asserts,
            const_statements,
            const_names,
        };

        if !ast_db.validate_section_name(0, output_nid, ast, diags) {
            bail!("AST construction failed");
        }

        if validate {
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
        }

        Ok(ast_db)
    }
}
