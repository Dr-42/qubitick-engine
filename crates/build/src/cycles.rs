//! Definition cycles: two nodes that define each other's glyphs.
//!
//! A definition that uses a glyph another definition declares is an ordinary
//! dependency — in a package it becomes an `import`. When that relation closes into
//! a cycle it is not an import problem, it is a defect in the corpus: the pair
//! defines each other, so at least one of them is circular. The per-file emitter
//! never noticed, because every file carried its own frame; a package does, because
//! an import cycle cannot exist.
//!
//! So this reports rather than repairs. Which of the two is load-bearing is a
//! mathematics decision, and merging them would hide it.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use qubitick_ctx::Ctx;

/// One member of a cycle, as a human would name it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CycleMember {
    /// The division number the node is cited by (`D.BT.37`).
    pub division: u32,
    pub name: String,
    /// The file that declares it.
    pub path: String,
}

/// A set of definitions that depend on each other in a closed loop.
#[derive(Debug, Clone)]
pub struct DefinitionCycle {
    /// The era the cycle lives in (definitions are per-era vocabulary).
    pub era: String,
    pub members: Vec<CycleMember>,
}

impl DefinitionCycle {
    /// `Era0/Strata0: D.BT.37 ContinuumStage <-> D.BT.48 Reals`
    pub fn message(&self) -> String {
        let members: Vec<String> = self
            .members
            .iter()
            .map(|m| format!("D.{}. {} ({})", m.division, m.name, m.path))
            .collect();
        format!("{}: {}", self.era, members.join(" <-> "))
    }
}

/// Every definition cycle in the corpus. Empty is good.
/// A cycle is a fact about the corpus, not about one scope: each file is parsed in ITS OWN scope
/// (the context here used to be handed in by the caller and ignored, which is the same
/// one-scope-for-everything assumption the pipeline had).
pub fn definition_cycles(conn: &qubitick_db::Db, corpus: &Path) -> Result<Vec<DefinitionCycle>> {
    // Parse each file once, in its own scope, and keep only its definition.
    struct Unit {
        era: String,
        member: CycleMember,
        defined: Vec<u32>,
        used: HashSet<u32>,
    }
    let mut units: Vec<Unit> = Vec::new();
    let mut ctxs: HashMap<String, Ctx> = HashMap::new();
    for file in crate::walk_qbit(corpus) {
        let scope = crate::pipeline::scope_of_path(&file);
        if !ctxs.contains_key(&scope) {
            ctxs.insert(scope.clone(), qubitick_ctx::build_ctx(conn, &scope)?);
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        /* A FILE THE CORPUS CANNOT READ HAS NO DEFINITIONS TO CYCLE. It is reported against
           itself by the build (`report.refused`, `coverage.unhomed`, `parse.not-notation`);
           aborting here would make one bad file hide every other file's answer. */
        let Ok(nodes) = qubitick_parse::parse_content(&ctxs[&scope], &text) else {
            continue;
        };
        for node in &nodes {
            let qubitick_model::Node::Definition(def) = node else {
                continue;
            };
            let Some(division) = crate::axioms::division_of_filename(&file.display().to_string())
            else {
                continue;
            };
            let mut used = HashSet::new();
            symbols_of(node, &mut used);
            units.push(Unit {
                era: scope.split('/').next().unwrap_or(&scope).to_string(),
                member: CycleMember {
                    division,
                    name: def.base.name.clone(),
                    path: file.display().to_string(),
                },
                defined: vec![def.symbol.primary],
                used,
            });
        }
    }

    // Symbol -> the unit that defines it (one era's vocabulary at a time).
    let mut definer: HashMap<(String, u32), usize> = HashMap::new();
    for (i, unit) in units.iter().enumerate() {
        for sym in &unit.defined {
            definer.insert((unit.era.clone(), *sym), i);
        }
    }
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (i, unit) in units.iter().enumerate() {
        for sym in &unit.used {
            if let Some(&j) = definer.get(&(unit.era.clone(), *sym))
                && j != i
            {
                edges.push((i, j));
            }
        }
    }

    let mut out: Vec<DefinitionCycle> = Vec::new();
    for group in components(units.len(), &edges) {
        if group.len() < 2 {
            continue; // a definition using its own glyph is recursion, not a cycle
        }
        let mut members: Vec<CycleMember> = group.iter().map(|&i| units[i].member.clone()).collect();
        members.sort();
        out.push(DefinitionCycle {
            era: units[group[0]].era.clone(),
            members,
        });
    }
    out.sort_by(|a, b| a.era.cmp(&b.era).then(a.members.cmp(&b.members)));
    Ok(out)
}

/// Strongly connected components of a directed graph (Tarjan, iterative — a corpus
/// is small, but recursion depth is not worth risking).
pub fn components(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (a, b) in edges {
        adj[*a].push(*b);
    }
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut next = 0usize;
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some((v, i)) = work.pop() {
            if i < adj[v].len() {
                let w = adj[v][i];
                work.push((v, i + 1));
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                if low[v] == index[v] {
                    let mut group = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        group.push(w);
                        if w == v {
                            break;
                        }
                    }
                    group.sort_unstable();
                    out.push(group);
                }
                if let Some(&(parent, _)) = work.last() {
                    low[parent] = low[parent].min(low[v]);
                }
            }
        }
    }
    out.sort_by_key(|g| g[0]);
    out
}

/// Every symbol a node mentions in its statements (its own definiendum included).
pub fn symbols_of(node: &qubitick_model::Node, out: &mut HashSet<u32>) {
    use qubitick_model::{Expr as E, FormBody, Node};
    fn walk(e: &E, out: &mut HashSet<u32>) {
        match e {
            E::Sym(ac) => {
                out.insert(ac.primary);
            }
            E::Var(_) => {}
            E::Apply(head, args) => {
                out.insert(head.primary);
                for a in args {
                    walk(a, out);
                }
            }
            E::Bracket(_, els) => els.iter().for_each(|a| walk(a, out)),
            E::Binder(_, binders, body) => {
                for b in binders {
                    walk(&b.ty, out);
                }
                walk(body, out);
            }
            E::Recursor(scrutinee, cases) => {
                walk(scrutinee, out);
                for (_, body) in cases {
                    walk(body, out);
                }
            }
            E::Matrix(rows) => rows.iter().for_each(|r| r.iter().for_each(|x| walk(x, out))),
            E::Vector(els) | E::Set(els) => els.iter().for_each(|a| walk(a, out)),
        }
    }
    match node {
        Node::Axiom(a) => a.statements.iter().for_each(|s| walk(s, out)),
        Node::Definition(d) => {
            if let Some(FormBody::MacroExpr(sema)) = &d.d_block.form {
                walk(sema, out);
            }
        }
        Node::Theorem(t) => walk(&t.statement, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_two_node_loop_is_one_component() {
        let edges = [(0, 1), (1, 0)];
        let groups = components(2, &edges);
        assert_eq!(groups, vec![vec![0, 1]]);
    }

    #[test]
    fn a_chain_is_all_singletons() {
        let edges = [(0, 1), (1, 2)];
        let groups = components(3, &edges);
        assert_eq!(groups, vec![vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn a_self_loop_is_a_singleton_component() {
        // recursion, not a mutual cycle: not reported
        let groups = components(1, &[(0, 0)]);
        assert_eq!(groups, vec![vec![0]]);
    }
}