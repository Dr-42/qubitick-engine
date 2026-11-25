//! The corpus's own typesetting, checked as typesetting.
//!
//! A `.qbit` file is LaTeX-shaped: glyph lines and proof rows are formulas that a
//! reader (and the render tile) sees as mathematics. Nothing checked that they
//! *are* LaTeX until now, so `\foobar` or an unbalanced brace reached the renderer
//! and became an error box — or, worse, silently rendered as literal text.
//!
//! Two rules keep this from crying wolf, because a warning that fires on correct
//! input is worse than no warning at all:
//!
//! 1. A command is "unknown" only if it is neither a **registered glyph** (some
//!    symbol's `tex` mentions it) nor in the whitelist of plain LaTeX below.
//!    `\mathbb`, `\frac` and `\forall` are not corpus symbols and never will be.
//! 2. Citation brackets are not mathematics. `[\ulcorner A1.1 b b a\urcorner]` is
//!    the corpus's citation syntax, and `\ulcornerA1.1` inside one is legitimate —
//!    checking bracket contents against a LaTeX vocabulary would flag 200 correct
//!    lines. Comments are skipped for the same reason.
//!
//! Everything here is a [`Severity::Warning`]: bad typesetting does not make the
//! Lean wrong, so it must not make the build red.

use std::collections::HashSet;
use std::path::Path;

use qubitick_ctx::Ctx;
use qubitick_model::{Diagnostic, Phase, Span};

/// Plain LaTeX / KaTeX that is formatting rather than corpus vocabulary.
///
/// Deliberately generous: it exists to *suppress* false positives, so an entry
/// that is never used costs nothing, while a missing one makes the linter lie.
const WHITELIST: &[&str] = &[
    /* the proof vocabulary: an admitted proof says `\sorry` (Lean's word, Isabelle's word) and
       the parser reads it as "no steps" — the emitter turns that theorem into an axiom, so it
       is corpus syntax, not a typesetting mistake to warn about */
    "sorry", "admit",
    // fonts, text, spacing
    "mathbb", "mathfrak", "mathcal", "mathbf", "mathsf", "mathit", "mathrm", "mathtt",
    "text", "textbf", "textit", "textrm", "operatorname", "textstyle", "displaystyle",
    "scriptstyle", "left", "right", "big", "Big", "bigg", "Bigg", "quad", "qquad",
    "hspace", "vspace", "phantom", "smash",
    // accents and decorations
    "hat", "widehat", "tilde", "widetilde", "bar", "overline", "underline", "vec",
    "dot", "ddot", "prime", "degree", "mod", "bmod", "pmod",
    // fractions, roots, delimiters
    "frac", "dfrac", "tfrac", "binom", "sqrt", "langle", "rangle", "vert", "Vert",
    "lvert", "rvert", "lVert", "rVert", "lceil", "rceil", "lfloor", "rfloor",
    "ulcorner", "urcorner", "llcorner", "lrcorner", "backslash",
    // relations and operators
    "to", "mapsto", "rightarrow", "leftarrow", "Rightarrow", "Leftarrow",
    "leftrightarrow", "Leftrightarrow", "longrightarrow", "longleftarrow",
    "Longrightarrow", "Longleftarrow", "iff", "implies", "ne", "neq", "le", "leq",
    "ge", "geq", "ll", "gg", "approx", "sim", "simeq", "cong", "equiv", "propto",
    "land", "lor", "wedge", "vee", "lnot", "neg", "forall", "exists", "nexists",
    "in", "notin", "ni", "subset", "subseteq", "subsetneq", "supset", "supseteq",
    "cup", "cap", "setminus", "emptyset", "varnothing", "infty", "partial", "nabla",
    "cdot", "times", "otimes", "oplus", "odot", "circ", "star", "ast", "dagger",
    "pm", "mp", "div", "sum", "prod", "int", "oint", "langle", "rangle", "mid",
    "vdash", "models", "top", "bot", "perp", "parallel", "therefore", "because",
    "ldots", "cdots", "vdots", "ddots", "dots", "aleph", "hbar", "ell", "Re", "Im",
    // Greek, including the variants KaTeX ships
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta",
    "theta", "vartheta", "iota", "kappa", "varkappa", "lambda", "mu", "nu", "xi",
    "pi", "varpi", "rho", "varrho", "sigma", "varsigma", "tau", "upsilon", "phi",
    "varphi", "chi", "psi", "omega", "Gamma", "Delta", "Theta", "Lambda", "Xi",
    "Pi", "Sigma", "Upsilon", "Phi", "Psi", "Omega", "varGamma", "varDelta",
    "varTheta", "varLambda", "varXi", "varPi", "varSigma", "varUpsilon", "varPhi",
    "varPsi", "varOmega",
    // function names — `\ln 2` in a calibration is mathematics, not vocabulary
    "ln", "log", "lg", "exp", "sin", "cos", "tan", "cot", "sec", "csc", "arcsin",
    "arccos", "arctan", "sinh", "cosh", "tanh", "coth", "det", "dim", "ker", "hom",
    "arg", "deg", "gcd", "lcm", "lim", "liminf", "limsup", "max", "min", "sup",
    "inf", "Pr", "mod", "bmod", "pmod",
    // more relations, operators, delimiters
    "implies", "impliedby", "doteq", "asymp", "bowtie", "dashv", "sqsubseteq",
    "sqsupseteq", "sqcup", "sqcap", "uplus", "amalg", "wr", "bigcup", "bigcap",
    "bigoplus", "bigotimes", "bigwedge", "bigvee", "coprod", "iint", "iiint",
    "lbrace", "rbrace", "lbrack", "rbrack", "colon", "hookrightarrow",
    "rightleftharpoons", "nearrow", "searrow", "swarrow", "nwarrow", "angle",
    "triangle", "square", "blacksquare", "checkmark", "bigcirc", "bullet",
    "diamond", "triangleleft", "triangleright", "leqslant", "geqslant", "lesssim",
    "gtrsim", "lessgtr", "prec", "succ", "preceq", "succeq", "not", "acute",
    "grave", "breve", "check", "mathring", "boldsymbol", "bm", "stackrel",
    "overset", "underset", "substack", "boxed", "cancel", "hfill", "newline",
    // environments
    "begin", "end", "textwidth", "linewidth", "arraystretch",
];

/// A prepared LaTeX lint: the vocabulary it accepts, built once per context.
pub struct LatexLint {
    known: HashSet<String>,
}

impl LatexLint {
    /// Every `\command` that a **declared** symbol's `tex` mentions, plus the
    /// whitelist.
    ///
    /// "Declared" is the load-bearing word. The ingester registers a glyph it has
    /// only ever *seen* as a use-only atom with no D-block, so taking the whole
    /// symbol table would make the check tautological: any command written anywhere
    /// would be "known", which is how `\frobnicate` passed a corpus-wide run
    /// silently. A symbol with an elaboration rule is vocabulary; one without is a
    /// name nobody defined — exactly what this is meant to notice.
    pub fn new(ctx: &Ctx) -> LatexLint {
        let mut known: HashSet<String> = WHITELIST.iter().map(|s| (*s).to_string()).collect();
        for row in ctx.symbols() {
            if row.elab.is_some() {
                collect_commands(&row.tex, &mut known);
            }
        }
        LatexLint { known }
    }

    pub fn knows(&self, command: &str) -> bool {
        self.known.contains(command)
    }

    /// Lint one `.qbit` source. Warnings only, in source order.
    pub fn lint(&self, path: &Path, src: &str) -> Vec<Diagnostic> {
        let file = path.display().to_string();
        let mut out = Vec::new();
        for (i, raw) in src.lines().enumerate() {
            let line = i as u32 + 1;
            let text = math_of(raw);
            if text.trim().is_empty() {
                continue;
            }
            // 1) every command must be one somebody knows
            for (command, col) in commands_in(&text) {
                if self.knows(&command) {
                    continue;
                }
                let mut d = Diagnostic::warning(
                    Phase::Latex,
                    "latex.unknown-command",
                    format!("`\\{command}` is not a registered glyph and not a known LaTeX command"),
                )
                .at(Span::at(&file, line, Some(col)));
                // `\ulcornerA1.1` is the corpus's citation form inside brackets, but
                // outside one it is a glyph name run into its argument — say so,
                // because that is the mistake this pattern actually is.
                if let Some(split) = split_glued(&command, &self.known) {
                    d = d.hint(format!(
                        "if the glyph is `\\{split}` followed by an argument, separate them: `\\{split} {rest}`",
                        rest = &command[split.len()..]
                    ));
                }
                out.push(d);
            }
            // 2) braces must balance on the line
            if let Some(col) = unbalanced_brace(&text) {
                out.push(
                    Diagnostic::warning(
                        Phase::Latex,
                        "latex.unbalanced-brace",
                        "unbalanced `{`/`}` braces",
                    )
                    .at(Span::at(&file, line, Some(col)))
                    .hint("the renderer will typeset from here to the end of the line as one group"),
                );
            }
            // 3) `$` must pair up (the corpus writes bare math, so this only fires
            //    when someone reaches for inline-math delimiters)
            let dollars = text.matches('$').count();
            if dollars % 2 == 1 {
                let col = text.find('$').map(|c| c as u32 + 1).unwrap_or(1);
                out.push(
                    Diagnostic::warning(
                        Phase::Latex,
                        "latex.unclosed-math",
                        format!("{dollars} `$` on this line: inline math never closes"),
                    )
                    .at(Span::at(&file, line, Some(col)))
                    .hint("write the formula without `$`, or close the pair"),
                );
            }
        }
        out
    }
}

/// The mathematical part of a corpus line: no comment, no citation brackets.
///
/// Citation brackets carry their own syntax (`[\ulcorner A1.1 b b a\urcorner]`), and
/// a comment is prose about the corpus — neither is mathematics, and both would
/// otherwise be reported as such.
fn math_of(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize; // inside [...]
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // keep the escape and whatever it escapes, so `\{` is not a brace —
                // but only outside a bracket, or `[\ulcorner A1\urcorner]` would leak
                // its commands back into the mathematics
                let next = chars.next();
                if depth == 0 {
                    out.push(c);
                    if let Some(n) = next {
                        out.push(n);
                    }
                }
            }
            '%' if depth == 0 => break,
            '[' => {
                depth += 1;
                if depth == 1 {
                    out.push(' ');
                }
            }
            ']' if depth > 0 => {
                depth -= 1;
            }
            _ if depth > 0 => {}
            _ => out.push(c),
        }
    }
    out
}

/// Every `\command` in `text`, with its 1-based column.
fn commands_in(text: &str) -> Vec<(String, u32)> {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != '\\' {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        if i >= bytes.len() {
            break;
        }
        if !bytes[i].is_ascii_alphabetic() {
            // `\,` `\;` `\{` — a one-character escape, not a command name
            i += 1;
            continue;
        }
        let mut name = String::new();
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            name.push(bytes[i]);
            i += 1;
        }
        out.push((name, start as u32 + 1));
    }
    out
}

fn collect_commands(tex: &str, into: &mut HashSet<String>) {
    for (cmd, _) in commands_in(tex) {
        into.insert(cmd);
    }
}

/// `\ulcornerA` → `Some("ulcorner")` when `ulcorner` is known and the rest is not
/// itself a command.
fn split_glued(command: &str, known: &HashSet<String>) -> Option<String> {
    if known.contains(command) {
        return None;
    }
    for cut in (1..command.len()).rev() {
        let head = &command[..cut];
        if known.contains(head) && command[cut..].chars().next().is_some_and(|c| !c.is_ascii_lowercase()) {
            return Some(head.to_string());
        }
    }
    None
}

/// The column of the first `}` that closes nothing, if the braces do not balance.
fn unbalanced_brace(text: &str) -> Option<u32> {
    let mut depth = 0i32;
    for (i, c) in text.chars().enumerate() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Some(i as u32 + 1);
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        // point at the last `{` that never closed
        let col = text.chars().rev().position(|c| c == '{')?;
        return Some((text.chars().count() - col) as u32);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::Severity;

    fn lint(body: &str) -> Vec<Diagnostic> {
        let ctx = qubitick_ctx::Ctx::from_rows("Era0/Strata0", Vec::new());
        LatexLint::new(&ctx).lint(Path::new("T.qbit"), body)
    }

    #[test]
    fn plain_latex_is_silent() {
        let out = lint("= \\forall a b : \\mathfrak{E}, (a \\Delta b) \\Delta c = a \\frac{1}{2} \\mathbb{Z}\n");
        assert!(out.is_empty(), "{out:#?}");
    }

    #[test]
    fn an_invented_command_is_reported_once_with_its_column() {
        let out = lint("= a \\frobnicate b\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code, "latex.unknown-command");
        assert_eq!(out[0].span.line, Some(1));
        assert!(out[0].message.contains("frobnicate"));
        assert_eq!(out[0].severity, Severity::Warning);
    }

    #[test]
    fn a_glyph_run_into_its_argument_gets_a_hint() {
        let out = lint("= \\ulcornerA1 b\\urcorner\n");
        assert_eq!(out.len(), 1);
        assert!(
            out[0].hints.iter().any(|h| h.contains("\\ulcorner A") || h.contains("\\ulcorner A1")),
            "{:?}",
            out[0].hints
        );
    }

    #[test]
    fn citation_brackets_and_comments_are_not_mathematics() {
        // the citation form glues the command to its label, and is correct there
        assert!(lint("  = a \\Delta (b \\Delta b)   [\\ulcornerA1.1 b b a\\urcorner]\n").is_empty());
        assert!(lint("% \\frobnicate is discussed here\n").is_empty());
    }

    #[test]
    fn braces_must_close_on_their_line() {
        let out = lint("= \\frac{1}{2\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].code, "latex.unbalanced-brace");
        let stray = lint("= a } b\n");
        assert_eq!(stray.len(), 1);
        assert_eq!(stray[0].span.col, Some(5));
    }

    #[test]
    fn an_odd_dollar_is_reported() {
        let out = lint("= $a \\Delta b\n");
        assert!(out.iter().any(|d| d.code == "latex.unclosed-math"), "{out:#?}");
    }

    #[test]
    fn an_escaped_brace_is_not_a_brace() {
        assert!(lint("= \\{ a \\}\n").is_empty());
    }
}