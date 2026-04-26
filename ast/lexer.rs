// Lexer for brink source files.
//
// This create replaces our previous use of the Logos lexer.  Switching
// to a hand-rolled lexer reduced many dependencies, some which had
// known security vulnerabilities.
//
// Provides the same three-method interface that the logos crate did:
//   Lexer::new(src)  — construct from source string
//   .next()          — advance and return the next LexToken (or None at EOF)
//   .slice()         — the source text of the most-recently returned token
//   .span()          — the byte range of the most-recently returned token
//

use crate::LexToken;

pub struct Lexer<'src> {
    src: &'src str,
    /// Current scan position (one past the end of tok_end after each call to next()).
    pos: usize,
    /// Start byte of the most-recently returned token.
    tok_start: usize,
    /// End byte (exclusive) of the most-recently returned token.
    tok_end: usize,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Lexer { src, pos: 0, tok_start: 0, tok_end: 0 }
    }

    /// The source text of the most-recently returned token.
    pub fn slice(&self) -> &'src str {
        &self.src[self.tok_start..self.tok_end]
    }

    /// The byte range of the most-recently returned token.
    pub fn span(&self) -> std::ops::Range<usize> {
        self.tok_start..self.tok_end
    }

    /// Advance to the next token and return it, or None at end of input.
    pub fn next(&mut self) -> Option<LexToken> {
        self.skip_nontokens();
        if self.pos >= self.src.len() {
            return None;
        }
        self.tok_start = self.pos;
        let tok = self.scan();
        self.tok_end = self.pos;
        Some(tok)
    }

    /// Skip all whitespace and comments, looping until nothing more can be skipped.
    fn skip_nontokens(&mut self) {
        loop {
            self.skip_whitespace();
            if !self.skip_line_comment() && !self.skip_block_comment() {
                break;
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.src.len() {
            match self.src.as_bytes()[self.pos] {
                b' ' | b'\t' | b'\n' | b'\x0C' => self.pos += 1,
                _ => break,
            }
        }
    }

    /// Returns true and advances past a `// ...` line comment, false otherwise.
    fn skip_line_comment(&mut self) -> bool {
        if !self.src[self.pos..].starts_with("//") {
            return false;
        }
        while self.pos < self.src.len() && self.src.as_bytes()[self.pos] != b'\n' {
            self.pos += 1;
        }
        true
    }

    /// Returns true and advances past a `/* ... */` block comment, false otherwise.
    fn skip_block_comment(&mut self) -> bool {
        if !self.src[self.pos..].starts_with("/*") {
            return false;
        }
        self.pos += 2;
        while self.pos + 1 < self.src.len() {
            if self.src.as_bytes()[self.pos] == b'*' && self.src.as_bytes()[self.pos + 1] == b'/' {
                self.pos += 2;
                return true;
            }
            self.pos += 1;
        }
        // Unterminated block comment: consume the rest and let the parser error.
        self.pos = self.src.len();
        true
    }

    // -----------------------------------------------------------------------
    // Token dispatch
    // -----------------------------------------------------------------------

    fn scan(&mut self) -> LexToken {
        let bytes = &self.src.as_bytes()[self.pos..];
        let b = bytes[0];

        if b == b'"' {
            return self.scan_string();
        }
        // Bare '-' followed immediately by [1-9] is an I64 literal.
        if b == b'-' && bytes.len() > 1 && bytes[1] >= b'1' && bytes[1] <= b'9' {
            return self.scan_negative_i64();
        }
        if b.is_ascii_digit() {
            return self.scan_number();
        }
        if b == b'_' || b.is_ascii_alphabetic() {
            return self.scan_word();
        }
        self.scan_operator()
    }

    // -----------------------------------------------------------------------
    // String literals
    // -----------------------------------------------------------------------

    /// Lex a double-quoted string, handling backslash escape sequences.
    fn scan_string(&mut self) -> LexToken {
        self.pos += 1; // opening "
        while self.pos < self.src.len() {
            let b = self.src.as_bytes()[self.pos];
            if b == b'\\' && self.pos + 1 < self.src.len() {
                self.pos += 2; // skip the escape character pair
            } else if b == b'"' {
                self.pos += 1; // closing "
                return LexToken::QuotedString;
            } else {
                self.pos += 1;
            }
        }
        LexToken::Unknown // unterminated string
    }

    // -----------------------------------------------------------------------
    // Numeric literals
    // -----------------------------------------------------------------------

    /// Lex `-[1-9][_0-9]*i?` — always an I64.
    fn scan_negative_i64(&mut self) -> LexToken {
        self.pos += 1; // '-'
        while self.pos < self.src.len()
            && (self.src.as_bytes()[self.pos].is_ascii_digit()
                || self.src.as_bytes()[self.pos] == b'_')
        {
            self.pos += 1;
        }
        if self.pos < self.src.len() && self.src.as_bytes()[self.pos] == b'i' {
            self.pos += 1;
        }
        LexToken::I64
    }

    /// Lex a numeric literal starting with [0-9].
    ///
    /// Decision tree:
    ///   0b/0B  → binary digits → suffix u→U64, i→I64, none→U64
    ///   0x/0X  → hex digits    → suffix u→U64, i→I64, none→U64
    ///   0u     → U64
    ///   0i     → I64
    ///   0      → Integer
    ///   [1-9]… → decimal       → suffix u→U64, i→I64, none→Integer
    fn scan_number(&mut self) -> LexToken {
        let bytes = self.src.as_bytes();

        if bytes[self.pos] == b'0' {
            let next = if self.pos + 1 < self.src.len() { bytes[self.pos + 1] } else { 0 };
            match next {
                b'b' | b'B' => {
                    self.pos += 2;
                    while self.pos < self.src.len()
                        && matches!(bytes[self.pos], b'0' | b'1' | b'_')
                    {
                        self.pos += 1;
                    }
                    return self.consume_binary_or_hex_suffix();
                }
                b'x' | b'X' => {
                    self.pos += 2;
                    while self.pos < self.src.len()
                        && (bytes[self.pos].is_ascii_hexdigit() || bytes[self.pos] == b'_')
                    {
                        self.pos += 1;
                    }
                    return self.consume_binary_or_hex_suffix();
                }
                b'u' => { self.pos += 2; return LexToken::U64; }
                b'i' => { self.pos += 2; return LexToken::I64; }
                _ => { self.pos += 1; return LexToken::Integer; } // bare `0`
            }
        }

        // [1-9][_0-9]*
        self.pos += 1;
        while self.pos < self.src.len()
            && (bytes[self.pos].is_ascii_digit() || bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }
        self.consume_decimal_suffix()
    }

    /// After consuming binary or hex digits, check for a u/i suffix.
    /// Default (no suffix) for binary/hex is U64.
    fn consume_binary_or_hex_suffix(&mut self) -> LexToken {
        if self.pos < self.src.len() {
            match self.src.as_bytes()[self.pos] {
                b'u' => { self.pos += 1; return LexToken::U64; }
                b'i' => { self.pos += 1; return LexToken::I64; }
                _ => {}
            }
        }
        LexToken::U64
    }

    /// After consuming decimal digits, check for a u/i suffix.
    /// Default (no suffix) for decimal is Integer.
    fn consume_decimal_suffix(&mut self) -> LexToken {
        if self.pos < self.src.len() {
            match self.src.as_bytes()[self.pos] {
                b'u' => { self.pos += 1; return LexToken::U64; }
                b'i' => { self.pos += 1; return LexToken::I64; }
                _ => {}
            }
        }
        LexToken::Integer
    }

    // -----------------------------------------------------------------------
    // Words: keywords, built-ins, identifiers, labels, namespaces
    // -----------------------------------------------------------------------

    /// Lex `[_a-zA-Z][0-9a-zA-Z_]*` then classify as keyword, Namespace,
    /// Label, or Identifier depending on trailing punctuation and keyword table.
    fn scan_word(&mut self) -> LexToken {
        let start = self.pos;
        while self.pos < self.src.len() {
            let b = self.src.as_bytes()[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }

        // Check for namespace (word::) or label (word:) suffix.
        let bytes = self.src.as_bytes();
        if self.pos + 1 < self.src.len() && bytes[self.pos] == b':' && bytes[self.pos + 1] == b':' {
            self.pos += 2;
            return LexToken::Namespace;
        }
        if self.pos < self.src.len() && bytes[self.pos] == b':' {
            self.pos += 1;
            return LexToken::Label;
        }

        match &self.src[start..self.pos] {
            "const"                  => LexToken::Const,
            "if"                     => LexToken::If,
            "else"                   => LexToken::Else,
            "__OUTPUT_SIZE"          => LexToken::BuiltinOutputSize,
            "__OUTPUT_ADDR"          => LexToken::BuiltinOutputAddr,
            "__BRINK_VERSION_STRING" => LexToken::BuiltinVersionString,
            "__BRINK_VERSION_MAJOR"  => LexToken::BuiltinVersionMajor,
            "__BRINK_VERSION_MINOR"  => LexToken::BuiltinVersionMinor,
            "__BRINK_VERSION_PATCH"  => LexToken::BuiltinVersionPatch,
            "include"                => LexToken::Include,
            "region"                 => LexToken::Region,
            "in"                     => LexToken::In,
            "section"                => LexToken::Section,
            "align"                  => LexToken::Align,
            "set_sec_offset"         => LexToken::SetSecOffset,
            "set_addr_offset"        => LexToken::SetAddrOffset,
            "set_addr"               => LexToken::SetAddr,
            "set_file_offset"        => LexToken::SetFileOffset,
            "assert"                 => LexToken::Assert,
            "sizeof"                 => LexToken::Sizeof,
            "print"                  => LexToken::Print,
            "to_u64"                 => LexToken::ToU64,
            "to_i64"                 => LexToken::ToI64,
            "addr"                   => LexToken::Addr,
            "addr_offset"            => LexToken::AddrOffset,
            "sec_offset"             => LexToken::SecOffset,
            "file_offset"            => LexToken::FileOffset,
            "wrs"                    => LexToken::Wrs,
            "wr8"                    => LexToken::Wr8,
            "wr16"                   => LexToken::Wr16,
            "wr24"                   => LexToken::Wr24,
            "wr32"                   => LexToken::Wr32,
            "wr40"                   => LexToken::Wr40,
            "wr48"                   => LexToken::Wr48,
            "wr56"                   => LexToken::Wr56,
            "wr64"                   => LexToken::Wr64,
            "wrf"                    => LexToken::Wrf,
            "wr"                     => LexToken::Wr,
            "output"                 => LexToken::Output,
            _                        => LexToken::Identifier,
        }
    }

    // -----------------------------------------------------------------------
    // Operators and punctuation
    // -----------------------------------------------------------------------

    /// Lex an operator or punctuation character.
    /// Multi-character operators are tried before their single-character prefixes.
    fn scan_operator(&mut self) -> LexToken {
        let rest = &self.src[self.pos..];

        // Two-character operators — must be checked before their single-char prefixes.
        if rest.starts_with("==") { self.pos += 2; return LexToken::DoubleEq; }
        if rest.starts_with("!=") { self.pos += 2; return LexToken::NEq; }
        if rest.starts_with(">=") { self.pos += 2; return LexToken::GEq; }
        if rest.starts_with("<=") { self.pos += 2; return LexToken::LEq; }
        if rest.starts_with("<<") { self.pos += 2; return LexToken::DoubleLess; }
        if rest.starts_with(">>") { self.pos += 2; return LexToken::DoubleGreater; }
        if rest.starts_with("&&") { self.pos += 2; return LexToken::DoubleAmpersand; }
        if rest.starts_with("||") { self.pos += 2; return LexToken::DoublePipe; }

        // Single-character operators and punctuation.
        self.pos += 1;
        match self.src.as_bytes()[self.pos - 1] {
            b'>' => LexToken::Gt,
            b'<' => LexToken::Lt,
            b'=' => LexToken::Eq,
            b'&' => LexToken::Ampersand,
            b'|' => LexToken::Pipe,
            b'+' => LexToken::Plus,
            b'-' => LexToken::Minus,
            b'*' => LexToken::Asterisk,
            b'/' => LexToken::FSlash,
            b'%' => LexToken::Percent,
            b',' => LexToken::Comma,
            b'{' => LexToken::OpenBrace,
            b'}' => LexToken::CloseBrace,
            b'(' => LexToken::OpenParen,
            b')' => LexToken::CloseParen,
            b';' => LexToken::Semicolon,
            _    => LexToken::Unknown,
        }
    }
}
