//! One diagnostic shape for the whole pipeline (LexParseAst §9.2, extended).
//!
//! Before this existed there were four: the parser had a rich [`Diagnostic`] with
//! severity, phase, span and hints; the build pipeline reported strings; the Lake
//! report reported strings; and the HTTP API reported one `{ok, error}` blob with
//! status 200. The cost was not stylistic — a warning had nowhere to go, so a
//! build could only be green or red, and nothing could point at a line.
//!
//! This is the shape every face emits: the CLI's JSON, the API's JSON, and what
//! the client puts in its diagnostics pane. Severity decides what a *reader* does
//! with it; only [`Severity::Error`] makes a build fail.

use serde::{Deserialize, Serialize};

/// How much a reader should care. Only `Error` fails a build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which stage produced it. A phase is what a reader needs to know *who* is
/// speaking: a parse error means fix the `.qbit`, a build error means the Lean
/// the emitter produced was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Lex,
    Parse,
    Elaborate,
    Scope,
    /// A citation names nothing the corpus has.
    Cite,
    Emit,
    /// Lean rejected the emitted module.
    Build,
    /// The corpus's own LaTeX does not look like LaTeX.
    Latex,
    Store,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Lex => "lex",
            Phase::Parse => "parse",
            Phase::Elaborate => "elaborate",
            Phase::Scope => "scope",
            Phase::Cite => "cite",
            Phase::Emit => "emit",
            Phase::Build => "build",
            Phase::Latex => "latex",
            Phase::Store => "store",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a diagnostic points.
///
/// `file` + `line` + `col` address the corpus source. `lean_line` addresses the
/// EMITTED module, which is a different document: the same defect has a line in
/// each, and pretending one number serves both is what made build failures
/// unclickable. A diagnostic carries the corpus position when it has one and the
/// Lean position when that is all that is known.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based line in `file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// 1-based column in `file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<u32>,
    /// Width of the underlined run, in characters. A caret line needs it: `^^^^` over
    /// the glyph a diagnostic is about is the difference between "somewhere in this
    /// file" and "this token".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub len: Option<u32>,
    /// 1-based line in the emitted Lean module.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lean_line: Option<u32>,
    /// 1-based column in the emitted Lean module.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lean_col: Option<u32>,
}

impl Span {
    pub fn file(path: impl Into<String>) -> Self {
        Span {
            file: Some(path.into()),
            ..Span::default()
        }
    }
    pub fn at(path: impl Into<String>, line: u32, col: Option<u32>) -> Self {
        Span {
            file: Some(path.into()),
            line: Some(line),
            col,
            len: None,
            lean_line: None,
            lean_col: None,
        }
    }
    /// Point at `len` characters starting at `col` — the run the caret line underlines.
    pub fn with_len(mut self, len: u32) -> Self {
        self.len = Some(len);
        self
    }
    pub fn is_empty(&self) -> bool {
        self.file.is_none()
            && self.line.is_none()
            && self.col.is_none()
            && self.len.is_none()
            && self.lean_line.is_none()
            && self.lean_col.is_none()
    }
}

impl std::fmt::Display for Span {
    /// `file:line:col`, or as much of it as is known.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.file, self.line, self.col) {
            (Some(file), Some(line), Some(col)) => write!(f, "{file}:{line}:{col}"),
            (Some(file), Some(line), None) => write!(f, "{file}:{line}"),
            (Some(file), None, _) => write!(f, "{file}"),
            (None, Some(line), _) => write!(f, "line {line}"),
            (None, None, _) => match self.lean_line {
                Some(l) => write!(f, "lean:{l}"),
                None => Ok(()),
            },
        }
    }
}

/// A diagnostic. `code` is the stable identity a tool keys on (`latex.unknown-command`);
/// `message` is for a human and may be reworded freely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub phase: Phase,
    pub code: String,
    pub span: Span,
    pub message: String,
    /// A few words under the caret, in rustc's shape: `^^^  used here`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `= help:` lines: what to DO about it. A fix belongs here and never in the
    /// title — a title that explains and advises is three sentences where a reader
    /// wanted one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// The corpus node this belongs to, when the emitter/assembler knows it. A
    /// Lean error arrives with a line in the emitted module and nothing else; the
    /// node is what a tile can highlight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        phase: Phase,
        code: &str,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            severity,
            phase,
            code: code.to_string(),
            span: Span::default(),
            message: message.into(),
            label: None,
            hints: Vec::new(),
            node: None,
        }
    }
    pub fn error(phase: Phase, code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Error, phase, code, message)
    }
    pub fn warning(phase: Phase, code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Warning, phase, code, message)
    }
    pub fn info(phase: Phase, code: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(Severity::Info, phase, code, message)
    }
    pub fn at(mut self, span: Span) -> Self {
        self.span = span;
        self
    }
    pub fn in_file(mut self, path: impl Into<String>) -> Self {
        self.span.file = Some(path.into());
        self
    }
    pub fn at_line(mut self, line: u32) -> Self {
        self.span.line = Some(line);
        self
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }
    /// A few words under the caret: what is wrong with THIS token.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    pub fn of_node(mut self, node: impl Into<String>) -> Self {
        self.node = Some(node.into());
        self
    }
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

/// Where a human renderer may find the source a span points into.
///
/// A diagnostic that says only `path:12` makes a reader open the file and count
/// lines; rustc and Lean both print the line, underline the run, and put the advice
/// underneath. This is the hook that lets them do the same without the model knowing
/// anything about a filesystem.
pub trait SourceLookup {
    /// The text of `file`, whole. A renderer slices the line it needs.
    fn source(&self, file: &str) -> Option<String>;
}

impl SourceLookup for std::collections::HashMap<String, String> {
    fn source(&self, file: &str) -> Option<String> {
        self.get(file).cloned()
    }
}

impl Diagnostic {
    /// Render in the shape rustc and Lean print: a title, an arrow to the position, the
    /// source line with the offending run underlined, then `= help:` lines.
    ///
    /// ```text
    /// error[coverage.unhomed]: `\Delta` is not declared in Era0/Strata0
    ///  --> Era0/Strata0/Axioms/A1_Base.qbit:1:38
    ///   |
    /// 1 | \forall a b : \mathfrak{E}, a \Delta b = b \Delta a
    ///   |                                  ^^^^^^^ used here
    ///   |
    ///   = help: add a D node whose glyph line is `a \Delta b`, with a `Type:` or `Sema:`
    /// ```
    ///
    /// A diagnostic with no line keeps the title and the arrow; a Lean position renders
    /// as the emitted module, because pretending a Lean line is a `.qbit` line sends a
    /// reader to the wrong file.
    pub fn render(&self, sources: &impl SourceLookup) -> String {
        let mut out = format!("{}[{}]: {}", self.severity, self.code, self.message);
        let (show_line, show_col) = match (&self.span.file, self.span.line) {
            (Some(_), Some(line)) => (Some(line), self.span.col),
            _ => (None, None),
        };
        match (&self.span.file, show_line) {
            (Some(file), Some(line)) => {
                out.push_str(&format!(
                    "\n --> {file}:{line}{}",
                    show_col.map(|c| format!(":{c}")).unwrap_or_default()
                ));
                if let Some(src) = sources.source(file) {
                    out.push_str(&snippet(&src, line, show_col, self.span.len, self.label.as_deref()));
                } else if let Some(label) = &self.label {
                    out.push_str(&format!("\n   | {label}"));
                }
            }
            (Some(file), None) => {
                // A Lean error knows a line in the EMITTED module and a circuitous route
                // back to the `.qbit`; saying so is better than pointing at the file and
                // letting a reader scan it.
                match (self.span.lean_line, self.span.lean_col) {
                    (Some(line), Some(col)) => {
                        out.push_str(&format!("\n --> emitted Lean for {file}, line {line}:{col}"))
                    }
                    (Some(line), None) => {
                        out.push_str(&format!("\n --> emitted Lean for {file}, line {line}"))
                    }
                    _ => out.push_str(&format!("\n --> {file}")),
                }
            }
            _ => {
                if let (Some(line), Some(col)) = (self.span.lean_line, self.span.lean_col) {
                    out.push_str(&format!("\n --> emitted Lean, line {line}:{col}"));
                } else if let Some(line) = self.span.lean_line {
                    out.push_str(&format!("\n --> emitted Lean, line {line}"));
                }
            }
        }
        if let Some(node) = &self.node {
            out.push_str(&format!("\n   = note: in node {node}"));
        }
        for hint in &self.hints {
            /* One prefix, ours. Lean's notes arrive as `Note: ...` and its suggestions as
               `Hint: ...`, and a rendered line that says `= help: Note: ...` is two
               prefixes where a reader wants one. */
            let text = hint
                .trim()
                .strip_prefix("Note: ")
                .or_else(|| hint.trim().strip_prefix("note: "))
                .or_else(|| hint.trim().strip_prefix("Hint: "))
                .or_else(|| hint.trim().strip_prefix("suggestion: "))
                .unwrap_or(hint.trim());
            out.push_str(&format!("\n   = help: {text}"));
        }
        out
    }
}

/// The source line, its number, and the caret run under it.
fn snippet(src: &str, line: u32, col: Option<u32>, len: Option<u32>, label: Option<&str>) -> String {
    let Some(text) = src.lines().nth(line.saturating_sub(1) as usize) else {
        return String::new();
    };
    let number = line.to_string();
    let pad = " ".repeat(number.len());
    let mut out = format!("\n{pad} |\n{number} | {text}");
    if let Some(col) = col {
        let col = (col.max(1) - 1) as usize;
        let width = len.unwrap_or(1).max(1) as usize;
        // A caret run may not run past the line it underlines.
        let avail = text.chars().count().saturating_sub(col);
        let width = width.min(avail.max(1));
        out.push_str(&format!(
            "\n{pad} | {}{}{}",
            " ".repeat(col),
            "^".repeat(width),
            label.map(|l| format!(" {l}")).unwrap_or_default()
        ));
    } else if let Some(label) = label {
        out.push_str(&format!("\n{pad} | {label}"));
    }
    out
}

impl std::fmt::Display for Diagnostic {
    /// `path:12: warning [latex.unknown-command] …` — one line, then hints, because
    /// a terminal reader needs the location first and the advice second.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let span = self.span.to_string();
        if span.is_empty() {
            write!(
                f,
                "{} [{}] {}",
                self.severity, self.code, self.message
            )?;
        } else {
            write!(
                f,
                "{}: {} [{}] {}",
                span, self.severity, self.code, self.message
            )?;
        }
        if let Some(node) = &self.node {
            write!(f, " (in {node})")?;
        }
        for hint in &self.hints {
            write!(f, "\n  hint: {hint}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_puts_the_location_first() {
        let d = Diagnostic::warning(Phase::Latex, "latex.unknown-command", "unknown \\foo")
            .at(Span::at("project/T.qbit", 7, Some(3)))
            .hint("did you mean \\forall");
        assert_eq!(
            d.to_string(),
            "project/T.qbit:7:3: warning [latex.unknown-command] unknown \\foo\n  hint: did you mean \\forall"
        );
    }

    #[test]
    fn a_lean_only_span_still_says_something() {
        let d = Diagnostic::error(Phase::Build, "build.lean-error", "unknown constant")
            .at(Span {
                lean_line: Some(42),
                ..Span::default()
            });
        assert_eq!(d.to_string(), "lean:42: error [build.lean-error] unknown constant");
    }

    #[test]
    fn only_errors_are_errors() {
        assert!(!Diagnostic::warning(Phase::Latex, "c", "m").is_error());
        assert!(Diagnostic::error(Phase::Build, "c", "m").is_error());
    }

    #[test]
    fn json_omits_what_is_unknown() {
        let d = Diagnostic::warning(Phase::Latex, "latex.test", "msg").in_file("x.qbit");
        let j = qubitick_json(&d);
        assert!(j.contains("\"file\":\"x.qbit\""));
        assert!(!j.contains("\"line\""));
        assert!(!j.contains("\"hints\""));
        assert!(j.contains("\"severity\":\"warning\""));
        assert!(j.contains("\"phase\":\"latex\""));
    }

    /* THE SHAPE IS THE STANDARD. rustc and Lean both print: a title, an arrow to the
       position, the source line with the run underlined, then `= help:`. A diagnostic
       that only says `path:12` costs the reader a file-open and a line count. */
    #[test]
    fn a_diagnostic_renders_in_rust_and_lean_shape() {
        let src = r"\forall a b : \mathfrak{E}, a \Delta b = b \Delta a";
        let d = Diagnostic::error(Phase::Scope, "coverage.unhomed", "`\\Delta` is not declared")
            .at(Span::at("A1_Base.qbit", 1, Some(29)).with_len(7))
            .label("used here")
            .hint("add a D node whose glyph line is `a \\Delta b`");
        let mut sources = std::collections::HashMap::new();
        sources.insert("A1_Base.qbit".to_string(), src.to_string());
        let rendered = d.render(&sources);
        let expected = concat!(
            "error[coverage.unhomed]: `\\Delta` is not declared\n",
            " --> A1_Base.qbit:1:29\n",
            "  |\n",
            "1 | \\forall a b : \\mathfrak{E}, a \\Delta b = b \\Delta a\n",
            "  |                             ^^^^^^^ used here\n",
            "   = help: add a D node whose glyph line is `a \\Delta b`"
        );
        assert_eq!(rendered, expected);
    }

    #[test]
    fn a_lean_position_says_it_is_the_emitted_module() {
        let d = Diagnostic::error(Phase::Build, "build.lean-error", "unknown identifier `R`").at(Span {
            lean_line: Some(12),
            lean_col: Some(5),
            ..Span::default()
        });
        assert_eq!(
            d.render(&std::collections::HashMap::new()),
            "error[build.lean-error]: unknown identifier `R`\n --> emitted Lean, line 12:5"
        );
    }

    #[test]
    fn a_title_with_hints_renders_without_a_position() {
        let d = Diagnostic::error(Phase::Store, "build.ingest-failed", "cannot read the corpus")
            .hint("check the path");
        assert_eq!(
            d.render(&std::collections::HashMap::new()),
            "error[build.ingest-failed]: cannot read the corpus\n   = help: check the path"
        );
    }

    fn qubitick_json(d: &Diagnostic) -> String {
        serde_json::to_string(d).expect("diagnostics serialize")
    }
}