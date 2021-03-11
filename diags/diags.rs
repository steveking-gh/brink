// codespan crate provide error reporting help
use codespan_reporting::diagnostic::{Diagnostic,Label};
use codespan_reporting::files::SimpleFile;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use std::ops::Range;

pub struct Diags<'a> {
    writer: StandardStream,
    source_map: SimpleFile<&'a str, &'a str>,
    config: codespan_reporting::term::Config,
    verbosity: u64,
    pub noprint: bool,
}

impl<'a, 'msg> Diags<'a> {
    pub fn new(name: &'a str, fstr: &'a str, verbosity: u64, noprint: bool) -> Self {
        Self {
            writer: StandardStream::stderr(ColorChoice::Always),
            source_map: SimpleFile::new(name,fstr),
            config: codespan_reporting::term::Config::default(),
            verbosity,
            noprint,
        }
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn warn(&self, code: &str, msg: &'msg str) {
        if self.verbosity == 0 { return; }

        let diag = Diagnostic::warning()
                .with_code(code)
                .with_message(msg);

        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, &diag);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn err0(&self, code: &str, msg: &'msg str) {
        if self.verbosity == 0 { return; }

        let diag = Diagnostic::error()
                .with_code(code)
                .with_message(msg);
        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, &diag);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn err1(&self, code: &str, msg: &'msg str,
                     loc: Range<usize>) {
        if self.verbosity == 0 { return; }

        let diag = Diagnostic::error()
                .with_code(code)
                .with_message(msg)
                .with_labels(vec![Label::primary((), loc)]);
        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, &diag);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn note0(&self, code: &str, msg: &'msg str) {
        if self.verbosity == 0 { return; }
        let diag = Diagnostic::note()
                .with_code(code)
                .with_message(msg);
        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, &diag);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn note1(&self, code: &str, msg: &'msg str,
                  loc: Range<usize>) {
        if self.verbosity == 0 { return; }

        let diag = Diagnostic::note()
                .with_code(code)
                .with_message(msg)
                .with_labels(vec![Label::primary((), loc)]);
        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, &diag);
    }

    /// Writes the diagnostic to the terminal with primary
    /// and secondary code locations.
    pub fn err2(&self, code: &str, msg: &'msg str,
                     loc1: Range<usize>,
                     loc2: Range<usize>) {
        if self.verbosity == 0 { return; }

        let diag = Diagnostic::error()
                .with_code(code)
                .with_message(msg)
                .with_labels(vec![Label::primary((), loc1),
                                  Label::secondary((), loc2)]);

        let _ = term::emit(&mut self.writer.lock(), &self.config,
                           &self.source_map, &diag);
    }
}
