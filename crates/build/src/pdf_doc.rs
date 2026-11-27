//! The corpus as a document, with its citations as LINKS.
//!
//! The PDF used to be a list of node headers — no statements, no proofs, nothing a citation could
//! point at, so "make cites hyperlink in the pdf" had nothing to work with. This module produces the
//! material for a real document: every node's own lines as the corpus writes them, and every
//! citation tag as a hyperlink to the target the node's own statement declares.
//!
//! ONE READING OF A CITATION, AND IT IS THE ENGINE'S. A tag is split the way the parser splits it
//! (`qubitick_parse::cite_name`) and resolved the way the build resolves it
//! (`crate::cite_target::resolve_cite`), so the key a target declares and the key a link writes are
//! the same string by construction — and the test counts them, because a document full of links to
//! nothing is worse than a document with no links.
//!
//! WHAT IS NOT LINKED: a local law (`\equiv`, `^i`) and a citation that names nothing. They print as
//! they are written; a link to nowhere would be a promise the document cannot keep.
//!
//! The typesetter is `qubitick_pdf`, which owns the preamble (and therefore `hyperref`) and the
//! assembly. This module owns what goes IN the document, and it lives here because reading the
//! corpus is what this crate does.

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_db::read::NodeHeader;
use qubitick_model::NodeKind;

/// One section of the document: a title and the LaTeX lines under it.
pub type PdfSection = (String, Vec<String>);

/// A line as the document prints it: prose, or one of the corpus's math lines.
///
/// The distinction matters in LaTeX, not in the corpus: prose is text mode (`\tau` in text mode is
/// an error) and a statement is display math.
const PROSE_LABELS: &[&str] = &["Math", "Phys", "Note"];

/// A line that OPENS with a label: `Type:`, `Sema:`, `Proof:`. The corpus's own shape (the same
/// rule `js/app/qbit-lines.js` uses for its `LABEL`), and it has to be anchored the same way: a
/// statement is free to contain a colon (`\forall a : \mathfrak{E}, …`), and reading that as a
/// label shifts every statement number after it by one — which moves every TARGET the document
/// declares, silently.
fn opens_with_label(line: &str) -> Option<(&str, &str)> {
    let (label, rest) = line.split_once(':')?;
    let label = label.trim();
    let mut chars = label.chars();
    let first = chars.next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphabetic()) || label.len() > 15 {
        return None;
    }
    Some((label, rest.trim()))
}

/// The whole corpus as sections, in the ledger order the interface lists it in.
pub fn sections(ctx: &Ctx, headers: &[NodeHeader]) -> Result<Vec<PdfSection>> {
    let mut out: Vec<PdfSection> = Vec::new();
    for header in headers {
        let Ok(src) = std::fs::read_to_string(&header.filename) else {
            /* a node whose file cannot be read still gets its heading: the document says what the
               corpus SAYS it has, and this is the honest half of it */
            out.push((heading(header), vec!["% this node's file could not be read".to_string()]));
            continue;
        };
        out.push((heading(header), lines_of(ctx, headers, header, &src)));
    }
    Ok(out)
}

/// The heading, WITH the node's own target: a citation may name the node itself (`T.1`, `D.2`), so
/// the target has to exist on the heading and not only on its statements.
fn heading(header: &NodeHeader) -> String {
    let label = crate::cite_target::label_of(header);
    format!(
        "\\hypertarget{{cite:{}}}{{}}{}",
        escape_key(&label),
        title(header)
    )
}

/// The heading text: what the node calls itself.
fn title(header: &NodeHeader) -> String {
    /* a plain colon, not an em dash: pdflatex with T1 does not know `—` and a document that fails
       to compile over a punctuation mark is a document nobody reads */
    let label = crate::cite_target::label_of(header);
    format!("{}: {}", label, escape_text(&header.name))
}

/// One node's source, as LaTeX lines with its citations linked.
fn lines_of(ctx: &Ctx, headers: &[NodeHeader], header: &NodeHeader, src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    /* the node's own statements need their own targets: a citation may name STATEMENT 4 of an axiom
       (`A1.4`), and that target has to exist on the statement itself */
    let mut statement: u32 = 0;
    /* THE FILE'S FIRST LINE IS THE NODE'S OWN HEADER (`T.BT.01. DeltaComm`), which the section
       heading already says: numbering it would declare a statement the corpus does not have, and
       (worse) shift every statement after it — moving every target the document declares. */
    let mut first = true;
    for raw in src.lines() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if first {
            first = false;
            continue;
        }
        if t.starts_with('%') {
            out.push(format!("% {}", escape_text(t.trim_start_matches('%').trim())));
            continue;
        }
        /* WHAT IS NOT ESCAPED IS THE MATHEMATICS. The corpus writes LaTeX, `crate::latex` lints it,
           and escaping it turns `\Delta` into a paragraph about a backslash — which is what the
           first version did, in the one path it did not think about (a line with no citation tag).
           Text-mode escaping is for names and prose. */
        let Some((label, rest)) = opens_with_label(t) else {
            /* a statement: number it, and give it a target */
            statement += 1;
            /* ONLY AN AXIOM'S STATEMENTS ARE NUMBERED: `A1.4` names statement 4 of an axiom, while
               `D.2` and `T.1` name a whole node — so a number beside a definition's statement would
               be a target the corpus's own vocabulary never uses, and an invitation to cite
               something that does not exist. */
            if header.node_type == NodeKind::Axiom {
                let anchor = crate::cite_target::label_of_line(header, statement);
                out.push(format!(
                    "\\noindent\\hypertarget{{cite:{}}}{{}}{}\\hfill [{}]",
                    escape_key(&anchor),
                    math(&row_markup(ctx, headers, t)),
                    escape_key(&anchor)
                ));
            } else {
                out.push(format!("\\noindent{}", math(&row_markup(ctx, headers, t))));
            }
            continue;
        };
        if PROSE_LABELS.contains(&label) {
            out.push(format!(
                "\\noindent\\textit{{{}}}: {}",
                escape_text(label),
                escape_text(rest.trim())
            ));
            continue;
        }
        /* a labelled line: a label with a value is mathematics (`Type: \tau \to \mathfrak{E}`), a
           label with nothing after it is just the label (`Proof:`), and both may carry a citation */
        /* DISPLAY MATHS IS NEVER PUT INSIDE A TEXT-MODE GROUP. `\texttt{Type}:\ [E]` is the
           shape that made pdflatex answer "Bad math environment delimiter": the `\[`
           follows a closing brace, in a paragraph that had already started, and TeX reads the
           `\` of `\[` as the line break `\\` followed by a stray `[`. Ending the text run
           with a blank line first gives the display its own paragraph, which is what the
           maths is. */
        if rest.trim().is_empty() {
            out.push(format!("\\noindent\\texttt{{{}}}:", escape_text(label)));
        } else {
            out.push(format!(
                "\\noindent\\texttt{{{}}}:

{}",
                escape_text(label),
                math(&row_markup(ctx, headers, rest.trim()))
            ));
        }
    }
    out
}

/// A maths line with its citation tags turned into links.
fn row_markup(ctx: &Ctx, headers: &[NodeHeader], text: &str) -> String {
    let Some(open) = text.rfind('[') else { return text.to_string() };
    if !text.trim_end().ends_with(']') {
        return text.to_string();
    }
    /* WHAT IS NOT ESCAPED IS THE MATHEMATICS: the corpus writes LaTeX, `crate::latex` lints it, and
       escaping a backslash would turn `\tau_0` into text about a backslash. Text-mode escaping is
       for the node's name and its prose lines. */
    let body = text[..open].trim_end();
    let inner = &text[open + 1..text.trim_end().len() - 1];
    let mut links: Vec<String> = Vec::new();
    for item in inner.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        match crate::cite_target::resolve_cite(ctx, headers, item) {
            /* the DISPLAY is what the author wrote (`\ulcornerA1.1 b b a\urcorner`); the KEY is
               what the target declares (`A1.1`) — one string each, from one resolution */
            /* the DISPLAY is the citation as the author wrote it, and it is inline MATHS: an
               instance (`\ulcornerA1.1 b b a\urcorner`) is written in maths symbols, which text
               mode rejects. */
            Some(target) => links.push(format!(
                "\\hyperlink{{cite:{}}}{{\\({}\\)}}",
                escape_key(&target.label),
                item
            )),
            None => links.push(format!("\\({}\\)", item)),
        }
    }
    let tag = if links.is_empty() { String::new() } else { format!("\\hfill [{}]", links.join(", ")) };
    format!("{body}{tag}")
}

/// Display maths. The corpus writes LaTeX, so the body goes in as it is written — what is escaped is
/// the text around it, never the mathematics (the corpus's own glyph vocabulary is what
/// `crate::latex` lints).
fn math(body: &str) -> String {
    format!("\\[{}\\]", body)
}

/// Text-mode escaping, for names and prose. A `_` in text mode is a LaTeX error, and a `%` comments
/// out the rest of the line — which would silently delete half a document.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '&' | '%' | '#' | '$' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            '_' => out.push_str("\\_"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            _ => out.push(c),
        }
    }
    out
}

/// A key inside `\hypertarget{cite:…}` / `\hyperlink{cite:…}`: dots are legal there, braces and
/// backslashes are not.
fn escape_key(s: &str) -> String {
    s.chars().filter(|c| *c != '\\' && *c != '{' && *c != '}').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_citation_is_a_link_to_the_key_its_target_declares() {
        let ctx = Ctx::from_rows("Era0/Strata0", vec![]);
        let axiom = NodeHeader {
            id: 1,
            node_type: NodeKind::Axiom,
            era: "Era0".into(),
            strata: 0,
            name: "Realm".into(),
            filename: "A1_Realm.qbit".into(),
            tag: None,
            version: 1,
            godel: None,
            authored: Some(1),
        };
        let headers = vec![axiom.clone()];

        /* the target the document declares on statement 4 of A1 */
        assert_eq!(crate::cite_target::label_of_line(&axiom, 4), "A1.4");

        /* and the link a row writes for it */
        let markup = row_markup(&ctx, &headers, "a \\Delta (a \\Delta a)    [A1.4]");
        assert!(markup.contains("\\hyperlink{cite:A1.4}{\\(A1.4\\)}"), "{markup}");
        assert!(markup.starts_with("a \\Delta (a \\Delta a)"), "the mathematics is not escaped: {markup}");
        assert!(markup.contains("\\hfill"), "the tag sits beside the row: {markup}");

        /* a local law is printed, not linked: there is nothing for it to point at */
        let local = row_markup(&ctx, &headers, "\\tau_0 = \\tau_0    [\\equiv]");
        assert!(!local.contains("\\hyperlink"), "{local}");
        assert!(local.contains("\\equiv"), "{local}");

        /* and a citation that names nothing is not a promise either */
        let dangling = row_markup(&ctx, &headers, "a = b    [T.9]");
        assert!(!dangling.contains("\\hyperlink"), "{dangling}");
        assert!(dangling.contains("T.9"), "{dangling}");
    }

    #[test]
    fn text_mode_escaping_survives_what_the_corpus_writes() {
        assert_eq!(escape_text("a_b"), "a\\_b");
        assert_eq!(escape_text("50% of it"), "50\\% of it");
        assert_eq!(escape_text("A1. Realm"), "A1. Realm");
        assert_eq!(escape_key("A1.4"), "A1.4");
    }
}
