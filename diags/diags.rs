use ariadne::{Color, Label, Report, ReportKind, Source};
use std::ops::Range;

pub struct Diags<'a> {
    name: &'a str,
    fstr: &'a str,
    verbosity: u64,
    pub noprint: bool,
}

impl<'a, 'msg> Diags<'a> {
    pub fn new(name: &'a str, fstr: &'a str, verbosity: u64, noprint: bool) -> Self {
        Self {
            name,
            fstr,
            verbosity,
            noprint,
        }
    }

    /// Helper to print ariadne reports
    fn print_report(&self, report: Report<(&'a str, Range<usize>)>) {
        if self.verbosity == 0 {
            return;
        }
        let _ = report.eprint((self.name, Source::from(self.fstr)));
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn warn(&self, code: &str, msg: &'msg str) {
        if self.verbosity == 0 {
            return;
        }

        let report = Report::build(ReportKind::Warning, self.name, 0)
            .with_code(code)
            .with_message(msg)
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn err0(&self, code: &str, msg: &'msg str) {
        if self.verbosity == 0 {
            return;
        }

        let report = Report::build(ReportKind::Error, self.name, 0)
            .with_code(code)
            .with_message(msg)
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn err1(&self, code: &str, msg: &'msg str, loc: Range<usize>) {
        if self.verbosity == 0 {
            return;
        }

        let start = loc.start;
        let report = Report::build(ReportKind::Error, self.name, start)
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((self.name, loc)).with_color(Color::Red))
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn note0(&self, code: &str, msg: &'msg str) {
        if self.verbosity == 0 {
            return;
        }

        let report = Report::build(ReportKind::Custom("Note", Color::Blue), self.name, 0)
            .with_code(code)
            .with_message(msg)
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// code location.
    pub fn note1(&self, code: &str, msg: &'msg str, loc: Range<usize>) {
        if self.verbosity == 0 {
            return;
        }

        let start = loc.start;
        let report = Report::build(ReportKind::Custom("Note", Color::Blue), self.name, start)
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((self.name, loc)).with_color(Color::Blue))
            .finish();
        self.print_report(report);
    }

    /// Writes the diagnostic to the terminal with primary
    /// and secondary code locations.
    pub fn err2(&self, code: &str, msg: &'msg str, loc1: Range<usize>, loc2: Range<usize>) {
        if self.verbosity == 0 {
            return;
        }

        let start = loc1.start;
        let report = Report::build(ReportKind::Error, self.name, start)
            .with_code(code)
            .with_message(msg)
            .with_label(Label::new((self.name, loc1)).with_color(Color::Red))
            .with_label(Label::new((self.name, loc2)).with_color(Color::Yellow))
            .finish();
        self.print_report(report);
    }
}
