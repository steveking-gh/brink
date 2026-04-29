// Diagnostic output for the brink compiler.
//
// Diags wraps the ariadne crate to produce richly formatted error, warning and
// note messages that point back to source locations in the original .brink
// file.  Every other pipeline stage receives a mutable Diags reference and
// calls err0/err1/err2, warn, or note0/note1 to report problems.  Diags does
// not make any pass/fail decisions; it only formats and emits messages.
//
// Order of operations: Diags is constructed first in process.rs and passed
// through every stage — ast, lineardb, irdb and engine — as the single
// channel through which all diagnostics flow.

use ariadne::{Color, Label, Report, ReportKind, sources};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub file_id: usize,
    pub range: Range<usize>,
}

pub struct Diags {
    name: String,
    pub files: Vec<(String, String)>,
    verbosity: u64,
    pub noprint: bool,
}

impl Diags {
    pub fn new(name: &str, fstr: &str, verbosity: u64, noprint: bool) -> Self {
        Self {
            name: name.to_string(),
            files: vec![(name.to_string(), fstr.to_string())],
            verbosity,
            noprint,
        }
    }

    /// Adds a new source file to the diagnostic reporting cache and returns
    /// its assigned unique `file_id`. The returned ID is intended to be embedded
    /// into `SourceSpan` objects, allowing error messages to print the correct
    /// file context and code snippets natively using `ariadne`.
    pub fn add_file(&mut self, name: &str, content: &str) -> usize {
        let id = self.files.len();
        self.files.push((name.to_string(), content.to_string()));
        id
    }

    /// Helper to print ariadne reports
    fn print_report(&self, report: Report<(String, Range<usize>)>) {
        if self.verbosity == 0 {
            return;
        }
        let cache = sources(self.files.clone());
        let _ = report.eprint(cache);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn warn(&self, code: &str, msg: &str) {
        if self.verbosity == 0 {
            return;
        }

        let report = Report::build(ReportKind::Warning, (self.name.clone(), 0..0))
            .with_code(code)
            .with_message(msg)
            .finish();
        self.print_report(report);
    }

    /// Writes a warning to the terminal with a primary source location.
    pub fn warn1(&self, code: &str, msg: &str, loc: SourceSpan) {
        if self.verbosity == 0 {
            return;
        }

        let id = self.files[loc.file_id].0.clone();
        let report = Report::build(ReportKind::Warning, (id.clone(), loc.range.clone()))
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((id, loc.range)).with_color(Color::Yellow))
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn err0(&self, code: &str, msg: &str) {
        if self.verbosity == 0 {
            return;
        }

        let report = Report::build(ReportKind::Error, (self.name.clone(), 0..0))
            .with_code(code)
            .with_message(msg)
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn err1(&self, code: &str, msg: &str, loc: SourceSpan) {
        if self.verbosity == 0 {
            return;
        }

        let id = self.files[loc.file_id].0.clone();
        let report = Report::build(ReportKind::Error, (id.clone(), loc.range.clone()))
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((id, loc.range)).with_color(Color::Red))
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn note0(&self, code: &str, msg: &str) {
        if self.verbosity == 0 {
            return;
        }

        let report = Report::build(
            ReportKind::Custom("Note", Color::Blue),
            (self.name.clone(), 0..0),
        )
        .with_code(code)
        .with_message(msg)
        .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn note1(&self, code: &str, msg: &str, loc: SourceSpan) {
        if self.verbosity == 0 {
            return;
        }

        let id = self.files[loc.file_id].0.clone();
        let report = Report::build(
            ReportKind::Custom("Note", Color::Blue),
            (id.clone(), loc.range.clone()),
        )
        .with_code(code)
        .with_message(msg)
        .with_label(Label::new((id, loc.range)).with_color(Color::Blue))
        .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with a primary location and N
    /// secondary locations, each carrying an individual annotation message.
    /// The primary label is red; each secondary label is yellow with its own
    /// message text embedded inline at the source site.
    pub fn err_with_locs(
        &self,
        code: &str,
        msg: &str,
        primary: SourceSpan,
        secondaries: &[(SourceSpan, String)],
    ) {
        if self.verbosity == 0 {
            return;
        }

        let pid = self.files[primary.file_id].0.clone();
        let mut builder = Report::build(ReportKind::Error, (pid.clone(), primary.range.clone()))
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((pid, primary.range)).with_color(Color::Red));
        for (loc, text) in secondaries {
            let sid = self.files[loc.file_id].0.clone();
            builder = builder.with_label(
                Label::new((sid, loc.range.clone()))
                    .with_color(Color::Yellow)
                    .with_message(text),
            );
        }
        self.print_report(builder.finish());
    }

    /// Writes the diagnostic to the terminal with primary
    /// and secondary code locations.
    pub fn err2(&self, code: &str, msg: &str, loc1: SourceSpan, loc2: SourceSpan) {
        if self.verbosity == 0 {
            return;
        }

        let id1 = self.files[loc1.file_id].0.clone();
        let id2 = self.files[loc2.file_id].0.clone();

        let report = Report::build(ReportKind::Error, (id1.clone(), loc1.range.clone()))
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((id1, loc1.range)).with_color(Color::Red))
            .with_label(Label::new((id2, loc2.range)).with_color(Color::Yellow))
            .finish();
        self.print_report(report);
    }
}
