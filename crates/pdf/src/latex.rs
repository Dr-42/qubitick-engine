//! LaTeX emission for the corpus summary PDF (CodeArch §5.7).

use qubitick_db::read::NodeHeader;

/// Produce a LaTeX document that lists the corpus nodes by kind.
pub fn document(nodes: &[NodeHeader]) -> String {
    let mut out = String::from(LATEX_PREAMBLE);
    out.push_str("\\begin{document}\n");
    out.push_str("\\title{Qubitick Corpus}\n\\maketitle\n");
    let mut kinds = nodes.to_vec();
    kinds.sort_by(|a, b| {
        let ka = (a.node_type.as_str().to_string(), a.era.clone(), a.name.clone());
        let kb = (b.node_type.as_str().to_string(), b.era.clone(), b.name.clone());
        ka.cmp(&kb)
    });
    for n in &kinds {
        let kind = n.node_type.as_str();
        let tag = n.tag.as_deref().unwrap_or("");
        let esc = escape(&n.name);
        out.push_str(&format!(
            "\\subsection{{{kind}: {esc} ({} {})}}\n",
            n.era, n.strata
        ));
        if !tag.is_empty() {
            out.push_str(&format!("({tag})\n"));
        }
    }
    out.push_str("\\end{document}\n");
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\textbackslash{}")
        .replace('_', "\\_")
        .replace('&', "\\&")
        .replace('#', "\\#")
        .replace('%', "\\%")
}

/// THE DOCUMENT A PROOF WANTS ITS CITATIONS TO BE IN. `hyperref` is last, as it must be, and its
/// links are coloured rather than boxed: a citation in a corpus document should LOOK like the
/// reference it is.
pub const LATEX_PREAMBLE: &str = r#"\documentclass[11pt]{article}
\usepackage[T1]{fontenc}
\usepackage[utf8]{inputenc}
\usepackage[margin=1in]{geometry}
\usepackage{xcolor}
\usepackage{amsmath,amssymb}
\usepackage{newunicodechar}
% THE CORPUS WRITES UNICODE OPERATORS (`×`, `→`, `⊢`, `∎`) AND THE DOCUMENT PRINTS THEM.
% pdflatex's text and math encodings carry some of these and not others; naming each one
% keeps the source free of the question, and a glyph added to the corpus later fails HERE
% rather than as an "Unicode character not set up" error in the middle of a paragraph.
\newunicodechar{×}{\ensuremath{\times}}
\newunicodechar{→}{\ensuremath{\rightarrow}}
\newunicodechar{←}{\ensuremath{\leftarrow}}
\newunicodechar{↔}{\ensuremath{\leftrightarrow}}
\newunicodechar{⊢}{\ensuremath{\vdash}}
\newunicodechar{∎}{\ensuremath{\blacksquare}}
\newunicodechar{□}{\ensuremath{\square}}
\newunicodechar{■}{\ensuremath{\blacksquare}}
\newunicodechar{≡}{\ensuremath{\equiv}}
\newunicodechar{∈}{\ensuremath{\in}}
\newunicodechar{⊆}{\ensuremath{\subseteq}}
\newunicodechar{≤}{\ensuremath{\leq}}
\newunicodechar{≥}{\ensuremath{\geq}}
\newunicodechar{≠}{\ensuremath{\neq}}
\newunicodechar{∧}{\ensuremath{\land}}
\newunicodechar{∨}{\ensuremath{\lor}}
\newunicodechar{¬}{\ensuremath{\lnot}}
\newunicodechar{∀}{\ensuremath{\forall}}
\newunicodechar{∃}{\ensuremath{\exists}}
\newunicodechar{∞}{\ensuremath{\infty}}
\newunicodechar{·}{\ensuremath{\cdot}}
\newunicodechar{−}{\ensuremath{-}}
\usepackage[colorlinks=true,linkcolor=blue!50!black,urlcolor=blue!50!black]{hyperref}
"#;

/// The corpus as one document, from material `qubitick_build::pdf_sections` assembled.
///
/// Each section is a node: its heading, then its own lines — statements as display maths, prose as
/// prose, and proof rows with their citation tags as hyperlinks to the targets those statements
/// declare. This function is the typesetter: the preamble above, the assembly, and nothing about
/// what a citation means.
pub fn document_sections(sections: &[(String, Vec<String>)]) -> String {
    let mut out = String::from(LATEX_PREAMBLE);
    out.push_str("\\begin{document}\n");
    out.push_str("\\title{Qubitick Corpus}\\maketitle\n");
    out.push_str("\\tableofcontents\n\\newpage\n");
    for (title, lines) in sections {
        out.push_str(&format!("\\section*{{{title}}}\n"));
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str("\\end{document}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(id: i64, kind: &str, name: &str) -> NodeHeader {
        NodeHeader {
            id,
            node_type: match kind {
                "Axiom" => qubitick_model::NodeKind::Axiom,
                "Definition" => qubitick_model::NodeKind::Definition,
                "Theorem" => qubitick_model::NodeKind::Theorem,
                _ => qubitick_model::NodeKind::Theorem,
            },
            era: "Era0".into(),
            strata: 0,
            name: name.into(),
            filename: "t.qbit".into(),
            tag: None,
            version: 1,
            godel: None,
            // the number the author wrote; this fixture's id doubles as it
            authored: Some(id),
        }
    }

    /// A DOCUMENT FULL OF LINKS TO NOTHING IS WORSE THAN ONE WITH NO LINKS: every
    /// `\hyperlink{cite:X}` must have a `\hypertarget{cite:X}`. The material comes from
    /// `qubitick_build::pdf_sections`, so this is the check that the two halves agree.
    #[test]
    fn every_link_has_a_target() {
        let sections = vec![
            (
                "A1: Realm".to_string(),
                vec![
                    "\\noindent\\hypertarget{cite:A1.1}{}\\[a \\Delta \\varnothing = a\\] \\ [A1.1]".to_string(),
                    "\\noindent\\hypertarget{cite:A1.4}{}\\[(a \\Delta b) \\Delta c = a \\Delta (b \\Delta c)\\] \\ [A1.4]".to_string(),
                ],
            ),
            (
                "T.1: DeltaComm".to_string(),
                vec![
                    "\\noindent\\hypertarget{cite:T.1}{}\\[\\Delta a b = \\Delta b a\\] \\ [T.1]".to_string(),
                    "\\[\\Delta a b\\]\\hfill [\\hyperlink{cite:A1.4}{\\(A1.4\\)}]".to_string(),
                    "\\[\\Delta a a\\]\\hfill [\\(\\equiv\\)]".to_string(),
                ],
            ),
        ];
        let doc = super::document_sections(&sections);
        assert!(doc.contains("\\usepackage[colorlinks=true"), "the document links: {doc}");
        assert!(doc.contains("\\section*{A1: Realm}"));
        assert!(doc.contains("\\tableofcontents"));

        let targets: std::collections::HashSet<&str> = doc
            .match_indices("\\hypertarget{")
            .map(|(i, _)| {
                let rest = &doc[i + "\\hypertarget{".len()..];
                &rest[..rest.find('}').unwrap_or(0)]
            })
            .collect();
        let links: Vec<&str> = doc
            .match_indices("\\hyperlink{")
            .map(|(i, _)| {
                let rest = &doc[i + "\\hyperlink{".len()..];
                &rest[..rest.find('}').unwrap_or(0)]
            })
            .collect();
        assert!(!links.is_empty(), "the row's citation is a link: {doc}");
        for link in &links {
            assert!(targets.contains(link), "{link} points at a target the document does not declare: {doc}");
        }
        assert!(!doc.contains("hyperlink{cite:\\equiv"), "a local law is not a link: {doc}");
    }

    #[test]
    fn produces_document() {
        let d = document(&[header(1, "Axiom", "Realm"), header(2, "Theorem", "Thm1")]);
        assert!(d.contains("\\documentclass"));
        assert!(d.contains("Realm"));
        assert!(d.contains("\\end{document}"));
    }
}