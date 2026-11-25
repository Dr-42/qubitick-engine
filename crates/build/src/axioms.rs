//! Load corpus axiom/definition/theorem nodes back out of the database.
//!
//! A theorem is emitted in a module that must also declare the axiom classes it
//! depends on and the previously-proven THEOREMS it cites (Document 5 §6.2/§6.3).
//! Because the corpus is one node per file, these are reconstructed here from the
//! ingested corpus DB (`node_statements` + `exprs`) and prepended before emission.

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_db::cache::ExprCache;
use qubitick_db::hash::hash_symbol;
use qubitick_db::read::{get_node_list, reconstruct_expr};
use qubitick_model::{Axiom, Era, Node, NodeBase, NodeKind, PhysOrMath, Scope, Strata};
use qubitick_model::corpus::Facet;

use std::collections::HashMap;

/// A corpus definition recovered from the DB, with its corpus division index.
///
/// The index is the `nn` of a `D_BT_<nn>_` filename, i.e. the number a proof
/// citation `[D.BT.<nn>]` names.
pub struct LoadedDefinition {
    pub division: u32,
    pub node: Node,
}

/// Reconstruct every `Definition` node from the corpus DB, keyed by the division
/// index parsed out of its filename.
pub fn load_definition_nodes(
    conn: &qubitick_db::Db,
    ctx: &Ctx,
    _scope: &str,
) -> Result<Vec<LoadedDefinition>> {
    let sym_hashes: HashMap<u32, u128> = ctx
        .symbols()
        .map(|r| (r.id as u32, hash_symbol(&r.scope, &r.tex, &r.kind)))
        .collect();
    let hash_of = |id: u32| sym_hashes.get(&id).copied().unwrap_or(0);
    let cache = ExprCache::new();

    let scope = ctx.scope().to_string();
    // A definition is cited BY ITS DIVISION NUMBER (`[D.BT.04]`), and each era
    // numbers its own vocabulary from 1 — so the same division can exist in an
    // inherited era AND in the citing file's era (Era0 defines `D.BT.04 Tick`,
    // Era1 defines its own `D.QB.04`). The era's own definition answers; an
    // inherited one fills in only numbers the own era does not claim. Without this
    // the rule table was keyed blind to era, and the last one loaded won.
    let mut ordered: Vec<_> = get_node_list(conn)?
        .into_iter()
        .filter(|h| {
            h.node_type == NodeKind::Definition && node_visible(ctx.scope(), &h.era, h.strata)
        })
        .collect();
    ordered.sort_by_key(|h| format!("{}/Strata{}", h.era, h.strata) != scope);

    let mut out = Vec::new();
    let mut seen_symbols: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut claimed_divisions: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for header in ordered {
        let Some(division) = division_of_filename(&header.filename) else {
            continue;
        };
        if !claimed_divisions.insert(division) {
            continue; // a nearer era already owns this division number
        }
        let mut statements = Vec::new();
        let mut line = 0i64;
        while let Some(root) = qubitick_db::queries::statement_root(conn, header.id, line)? {
            let expr = reconstruct_expr(conn, &cache, root, &hash_of)?;
            statements.push((*expr).clone());
            line += 1;
        }
        // A definition's Sema is stored as the node's statement; the glyph it
        // defines is the statement's head symbol.
        let Some(sema) = statements.first().cloned() else {
            continue;
        };
        // The defined glyph is the DEFININIENDUM's head. When the definiendum is a
        // compound (e.g. `e_1 \cap e_2`), the head is an OPERATOR that is NOT the
        // glyph being defined — skip it so it doesn't pollute `defined_names` and
        // evict `cap`/`Delta` from the frame (which broke theorem modules).
        let symbol = definiendum_head(ctx, &sema);
        // Skip operator-headed misparses, unidentifiable (symbol 0) glyphs, and
        // duplicate glyphs claimed by more than one definition.
        if is_compound_definiendum_operator(ctx, &sema)
            || symbol == 0
            || !seen_symbols.insert(symbol)
        {
            continue;
        }
        out.push(LoadedDefinition {
            division,
            node: Node::Definition(qubitick_model::Definition {
                base: NodeBase {
                    id: header.id as u32,
                    kind: NodeKind::Definition,
                    era: Era::Era0,
                    strata: Strata(0),
                    name: header.name.clone(),
                    req: Vec::new(),
                    derived: Vec::new(),
                    note: None,
                },
                symbol: qubitick_model::AtomicComposite::new(symbol),
                extra_symbols: Vec::new(),
                d_block: qubitick_model::DBlockIR {
                    kind: qubitick_model::SymbolKind::Notation,
                    sig: None,
                    form: Some(qubitick_model::FormBody::MacroExpr(sema)),
                    ..Default::default()
                },
                tag: PhysOrMath::Math,
            }),
        });
    }
    Ok(out)
}

/// The head symbol of a definition's definiendum.
///
/// `Eq(glyph args, body)` defines `glyph`; a non-equation Sema is taken at face
/// value.
/// Whether a Sema's definiendum is a compound whose head is an OPERATOR used
/// across the corpus (so the defined glyph is actually a subject, not that
/// operator). Handles `e_1 \cap e_2 = ∅` style rows via the context's tex.
fn is_compound_definiendum_operator(ctx: &Ctx, sema: &qubitick_model::Expr) -> bool {
    use qubitick_model::Expr;
    let target = match sema {
        Expr::Apply(h, args) if h.primary == qubitick_model::EQ_SYM && args.len() == 2 => &args[0],
        other => other,
    };
    let Expr::Apply(h, _) = target else {
        return false;
    };
    let Some(tex) = ctx.by_id(h.primary as i64).map(|r| r.tex.clone()) else {
        return false;
    };
    // Deliberately broader than the emitter's [`corpus::is_operand_postulate`]: this
    // one also refuses a `\land`-joined Sema (`D.BT.56`'s recursive tick operators),
    // whose head is `\land`, because `definiendum_head` cannot name a definiendum for
    // them and would answer `\land` — the operator the whole corpus is written with.
    // Use the hardcoded PUBLIC_SYMBOL_CLASSES table only: this function is meant to
    // skip definitions for BUILTIN operators (like \cap, \times) that are already in
    // the frame. Project-declared operators (like +, ×) must NOT be skipped here,
    // even if they have BinOp facet in the database, because they ARE the operator
    // being defined.
    qubitick_model::corpus::is_class(&tex, Facet::BinOp)
}

/// The head symbol of a definition's definiendum.
///
/// A Sema is `Eq(glyph args, body)` or `Iff(glyph args, body)`. The DEFINED glyph
/// is the head of the compound definiendum on the left — peel top-level `=`/`\leftrightarrow`
/// connectives into it. So `a \ne b \leftrightarrow …` defines `\ne` (not the
/// `\leftrightarrow` connective that only heads the equation).
fn definiendum_head(ctx: &Ctx, sema: &qubitick_model::Expr) -> u32 {
    // The rule itself lives in `qubitick_model::corpus`, because the PARSER needs
    // the very same peel: a definition must resolve to one symbol id whether it is
    // rebuilt from the DB or parsed from its file.
    qubitick_model::corpus::definiendum_head(sema, |id| {
        ctx.by_id(id as i64).is_some_and(|r| {
            ctx.has_facet(&r.tex, Facet::LeftIff)
        })
    })
}

/// Whether a corpus node living at `EraN/StrataM` is visible to a file at `scope`
/// (DataSchema §2 via `Scope::visible_from`: own scope, ancestor, or same-era
/// earlier strata). Filters the context loaders so one era does not leak another.
pub(crate) fn node_visible(scope: &str, era: &str, strata: i64) -> bool {
    Scope(scope.to_string()).visible_from(&Scope(format!("{era}/Strata{strata}")))
}

/// The division index in a corpus filename such as `D_BT_02_Neg.qbit`.
pub fn division_of_filename(filename: &str) -> Option<u32> {
    let stem = filename.rsplit('/').next().unwrap_or(filename);
    // Skip the leading kind tag (`D_`, `T_`, `A_`) and the division tag
    // (`BT_`, `QB_`, ...) to the first digit run: `D_BT_02_Neg` -> 2,
    // `T_BT_01_DeltaComm` -> 1.
    let idx = stem.find(|c: char| c.is_ascii_digit())?;
    let digits: String = stem[idx..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Reconstruct every `Axiom` node from the corpus DB, in node-id (ledger) order.
///
/// Statements are read back from `node_statements` line by line and rebuilt with
/// the hash-consing `reconstruct_expr`, so the axioms arrive with the same
/// structure they were ingested with.
pub fn load_axiom_nodes(conn: &qubitick_db::Db, ctx: &Ctx, _scope: &str) -> Result<Vec<Node>> {
    // Symbol id → build-independent hash, exactly as ingest computed them.
    let sym_hashes: HashMap<u32, u128> = ctx
        .symbols()
        .map(|r| (r.id as u32, hash_symbol(&r.scope, &r.tex, &r.kind)))
        .collect();
    let hash_of = |id: u32| sym_hashes.get(&id).copied().unwrap_or(0);

    let cache = ExprCache::new();
    let scope = ctx.scope().to_string();
    // Citations name the number the AUTHOR wrote (`[A4.2]` = axiom 4, statement 2),
    // and that number is only unique WITHIN a scope: Era0 has an A1, and so may a
    // later era. The file's own era answers first; an inherited era answers only for
    // numbers the own era does not claim. Keyed on the ledger ordinal instead (the
    // old behaviour) any insertion renumbered every axiom after it, and a citation
    // to a number that no longer existed still resolved — to the wrong axiom.
    let mut ordered: Vec<_> = get_node_list(conn)?
        .into_iter()
        .filter(|h| h.node_type == NodeKind::Axiom && node_visible(ctx.scope(), &h.era, h.strata))
        .collect();
    // stable: within each group the ledger order is kept
    ordered.sort_by_key(|h| format!("{}/Strata{}", h.era, h.strata) != scope);
    let mut claimed: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for header in ordered {
        let authored = header.authored.unwrap_or(header.id) as u32;
        if !claimed.insert(authored) {
            continue; // a nearer era already owns this number
        }
        let mut statements = Vec::new();
        let mut line = 0i64;
        // Statements are stored under consecutive line indices; stop at the first
        // gap (or use the recorded count when available).
        while let Some(root) = qubitick_db::queries::statement_root(conn, header.id, line)? {
            let expr = reconstruct_expr(conn, &cache, root, &hash_of)?;
            statements.push((*expr).clone());
            line += 1;
        }
        out.push(Node::Axiom(Axiom {
            base: NodeBase {
                id: authored,
                kind: NodeKind::Axiom,
                era: Era::Era0,
                strata: Strata(0),
                name: header.name.clone(),
                req: Vec::new(),
                derived: Vec::new(),
                note: None,
            },
            statements,
            tag: PhysOrMath::Math,
            falsify: None,
        }));
    }
    Ok(out)
}

/// A previously-proven theorem that a proof cites, keyed by its corpus division
/// index (the `nn` of `T_BT_<nn>`).
#[derive(Debug, Clone)]
pub struct CitedTheorem {
    pub division: u32,
    pub name: String,
    pub statement: qubitick_model::Expr,
}

/// Reconstruct, from the DB, the theorem `division` (statement + name) so it can
/// be declared as an assumed lemma in a module that cites it.
pub fn load_theorem(
    conn: &qubitick_db::Db,
    ctx: &Ctx,
    _scope: &str,
    division: u32,
) -> Result<Option<CitedTheorem>> {
    let sym_hashes: std::collections::HashMap<u32, u128> = ctx
        .symbols()
        .map(|r| (r.id as u32, qubitick_db::hash::hash_symbol(&r.scope, &r.tex, &r.kind)))
        .collect();
    let hash_of = |id: u32| sym_hashes.get(&id).copied().unwrap_or(0);
    let cache = ExprCache::new();
    for header in get_node_list(conn)? {
        if header.node_type != NodeKind::Theorem {
            continue;
        }
        if !node_visible(ctx.scope(), &header.era, header.strata) {
            continue;
        }
        let Some(d) = division_of_filename(&header.filename) else {
            continue;
        };
        if d != division {
            continue;
        }
        let Some(root) = qubitick_db::queries::statement_root(conn, header.id, 0)? else {
            continue;
        };
        let expr = reconstruct_expr(conn, &cache, root, &hash_of)?;
        return Ok(Some(CitedTheorem {
            division,
            name: header.name.clone(),
            statement: (*expr).clone(),
        }));
    }
    Ok(None)
}

/// Collect the `T_BT.<n>` division indices a theorem's proof cites, in order.
pub fn cited_divisions(nodes: &[qubitick_model::Node]) -> Vec<u32> {
    use qubitick_model::{ChainJust, CiteName, Node, ProofStep};
    fn walk(steps: &[ProofStep], out: &mut Vec<u32>) {
        for step in steps {
            match step {
                ProofStep::Chain { rows } => {
                    for row in rows {
                        if let ChainJust::Rewrite { refs } = &row.just {
                            for r in refs {
                                if let CiteName::TheoremId(div) = r.name
                                    && !out.contains(&div)
                                {
                                    out.push(div);
                                }
                            }
                        }
                    }
                }
                // A theorem's proof is frequently an INDUCTION whose case bodies
                // hold the citations. Scanning only top-level chains missed every
                // `[T.*]` inside a `\mu` block, so the cited theorem was never
                // loaded as a lemma and the row failed with
                // "T.10 (not in this module)".
                ProofStep::Induction { cases, .. } => {
                    for c in cases {
                        walk(&c.body, out);
                    }
                }
                ProofStep::Note { body, .. } => walk(body, out),
                ProofStep::Recursor { cases, .. } => {
                    for c in cases {
                        walk(&c.body, out);
                    }
                }
                ProofStep::Case { arms, .. } => {
                    for a in arms {
                        walk(&a.body, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for n in nodes {
        if let Node::Theorem(t) = n {
            walk(&t.proof, &mut out);
        }
    }
    out
}
