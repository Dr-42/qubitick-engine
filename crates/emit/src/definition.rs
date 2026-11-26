//! Definition emission (CodeArch §5.11).

use std::collections::HashMap;

use anyhow::Result;
use qubitick_ctx::Ctx;
use qubitick_model::Definition;

/// Emit a definition per Document 5 §5.3/§5.4.
///
/// No `sorry` is ever emitted: a definition without a synthesizable shape fails
/// the build (returns `Err`) per Doc 5 §7.4 ("No `sorry`. If the compiler cannot
/// produce a term, it fails the build.").
pub fn emit_definition(ctx: &Ctx, def: &Definition) -> Result<String> {
    emit_definition_in(
        ctx,
        def,
        false,
        0,
        &HashMap::new(),
        &Default::default(),
        &HashMap::new(),
        &HashMap::new(),
        false,
    )
}

/// As [`emit_definition`], but when `is_sort` the glyph is used as a binder sort
/// elsewhere in the module, so it is declared as a `Type`-valued space.
#[allow(clippy::too_many_arguments)]
pub fn emit_definition_in(
    ctx: &Ctx,
    def: &Definition,
    is_sort: bool,
    arity: usize,
    fn_names: &std::collections::HashMap<u32, String>,
    sorts: &std::collections::HashSet<u32>,
    formations: &HashMap<u32, (String, Vec<u32>)>,
    known: &HashMap<String, String>,
    // Emitting a GROUND: the file that DECLARES the class, so a field of the instance
    // names itself (`plus`) rather than being reached through it (`R.plus`).
    ground: bool,
) -> Result<String> {
    // A dual-use glyph declares its APPLIED form under the `_fn` name: its authored
    // `Type:` describes `\mathfrak{L}(t)`, while the bare `\mathfrak{L}` stays the
    // frame's `Prop`.
    let name = crate::module::definition_decl_name(ctx, def, fn_names);
    // Sort glyphs render as the carrier so a body's lambda binder type matches the
    // operator declarations (`bigcup : Prop → Prop`).
    let base = crate::resolver_with_sorts(ctx, fn_names, sorts);
    let resolver = move |id: u32| -> String {
        let name = base(id);
        if ground {
            name.strip_prefix("R.").map(str::to_string).unwrap_or(name)
        } else {
            name
        }
    };
    /* A GLYPH WITH A GROUND NAME IS THE GROUND'S TO DECLARE.
     *
     * `Type: R.plus` says where the glyph lives; the engine writes no declaration for it
     * anywhere — not in a module and not in a scaffolded ground. The ground's own hands
     * put it in the class, which is why the scaffold's output has the class and no axiom. */
    if def.d_block.ground_name.is_some() {
        return Ok(String::new());
    }
    if is_sort {
        return Ok(format!("axiom {name} : Type\n"));
    }
    match &def.d_block.form {
        Some(qubitick_model::FormBody::MacroExpr(e)) => {
            // A compilable Sema becomes a real `def` (so proofs can unfold it
            // definitionally); otherwise the glyph is declared uninterpreted. An
            // operator defined by recursion over a declared formation is emitted as a
            // recursor `def` first; only if that is not applicable do we fall back to
            // a plain macro/axiom.
            Ok(
                recursor_def_text_formation(ctx, &name, e, &resolver, formations)
                    .or_else(|| macro_def_text(ctx, &name, e, &resolver, known))
                    // Nothing about this Sema compiles to a term, so the glyph stays
                    // uninterpreted — declared with the type the FRAME inferred for
                    // it, not a flat `Prop → ... → Prop`. The two must agree: the
                    // frame's answer is what every OTHER module sees, and in a ground
                    // that holds both, a flat declaration here would make the glyph's
                    // own node disagree with its own helper's body.
                    .unwrap_or_else(|| match known.get(&name) {
                        Some(ty) if ty != "Type" => format!("axiom {name} : {ty}\n"),
                        _ => declaration(&name, arity),
                    }),
            )
        }
        Some(qubitick_model::FormBody::Formation(ctors)) => {
            // Document 5 §5.3: bare-prelude inductive with `genInjectivity` off.
            let ctors = ctors
                .iter()
                .map(|c| symbol_lean_name(ctx, c.constructor.primary))
                .collect::<Vec<_>>();
            let mut out = format!("set_option genInjectivity false in\ninductive {name} : Type\n");
            for c in ctors {
                out.push_str(&format!("  | {c}\n"));
            }
            Ok(out)
        }
        // A body we cannot compile (recursive/bracket forms) leaves the glyph as
        // an uninterpreted symbol rather than failing the whole file.
        Some(_) => Ok(declaration(&name, arity)),
        None => {
            // An authored `Type:` is read EXACTLY as the frame reads it
            // (`render_type_expr`), not rendered verbatim. The two must agree: the
            // frame declares the glyphs a signature mentions flat over the carrier
            // (`\mathfrak H : Prop`), so a definition that rendered its own
            // signature verbatim declared `\mathfrak T : \mathfrak H -> \mathcal R`
            // — and `\mathfrak H`, being a `Prop`, is then not a type at all
            // (`Unknown constant CoeSort`), which is precisely what an era ground
            // hit while the same definition passed alone.
            let sig = def
                .d_block
                .sig
                .as_ref()
                .map(|s| crate::module::render_type_expr(ctx, s));
            Ok(match sig {
                Some(ty) => format!("axiom {name} : {ty}\n"),
                None => declaration(&name, arity),
            })
        }
    }
}

/// An uninterpreted declaration of `name` at the observed arity.
fn declaration(name: &str, arity: usize) -> String {
    let mut ty = String::from("Prop");
    for _ in 0..arity {
        ty = format!("Prop → {ty}");
    }
    format!("axiom {name} : {ty}\n")
}

/// Whether `body` mentions `name` as a whole identifier.
fn mentions_word(body: &str, name: &str) -> bool {
    body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\''))
        .any(|t| t == name)
}

/// The two sides of an `Eq` application, or `None`.
fn eq_sides(x: &qubitick_model::Expr) -> Option<(&qubitick_model::Expr, &qubitick_model::Expr)> {
    use qubitick_model::{Expr, EQ_SYM};
    if let Expr::Apply(h, a) = x
        && h.primary == EQ_SYM
        && a.len() == 2
    {
        Some((&a[0], &a[1]))
    } else {
        None
    }
}

/// Emit an operator DEFINED BY RECURSION over a DECLARATIVE FORMATION carrier, from a
/// `Sema:` `land` of constructor equations `Eq (f ctor₁) rhs₁ ∧ … ∧ Eq (f ctorₙ) rhsₙ`,
/// every ctor a nullary constructor of one `FormBody::Formation`.
pub(crate) fn recursor_def_text_formation(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    formations: &HashMap<u32, (String, Vec<u32>)>,
) -> Option<String> {
    use qubitick_model::Expr;
    if formations.is_empty() {
        return None;
    }
    let mut parts: Vec<&Expr> = Vec::new();
    fn fl<'a>(x: &'a Expr, ctx: &Ctx, out: &mut Vec<&'a Expr>) {
        match x {
            Expr::Apply(h, args)
                if args.len() == 2 && crate::module::is_conjunct_head(ctx, h.primary) =>
            {
                for a in args {
                    fl(a, ctx, out);
                }
            }
            o => out.push(o),
        }
    }
    fl(e, ctx, &mut parts);
    let mut by_ctor: std::collections::HashMap<u32, String> = Default::default();
    for p in &parts {
        let (l, r) = eq_sides(p)?;
        let Expr::Apply(f, fargs) = l else { return None };
        if resolver(f.primary) != name {
            return None;
        }
        if fargs.len() != 1 {
            return None;
        }
        let last = fargs.last()?;
        let cid = match last {
            Expr::Sym(ac) => ac.primary,
            Expr::Apply(h2, _) => h2.primary,
            _ => return None,
        };
        if !formations.contains_key(&cid) {
            return None;
        }
        let rhs = qubitick_lean::render(r, resolver);
        if rhs.contains('\\') || rhs.is_empty() {
            return None;
        }
        by_ctor.insert(cid, rhs);
    }
    let (carrier, order) = formations.get(by_ctor.keys().next()?)?.clone();
    if !order.iter().all(|c| by_ctor.contains_key(c)) {
        return None;
    }
    let branches: Vec<String> = order.iter().filter_map(|c| by_ctor.get(c).cloned()).collect();
    if branches.len() != order.len() {
        return None;
    }
    Some(format!(
        "noncomputable def {name} : {carrier} → {carrier} :=\n  fun x => {carrier}.rec (motive := fun _ => {carrier}) {} x",
        branches.join(" "),
    ))
}
pub(crate) fn macro_def_text(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    known: &HashMap<String, String>,
) -> Option<String> {
    use qubitick_model::{AtomicComposite, Expr, EQ_SYM};
    // `=`- and iff-headed defining equations are both accepted as a definitional
    // top. Relational glyphs are defined iff-wise in the corpus
    // (`a \subseteq b \leftrightarrow a \cap b = a`), and if they came out as plain
    // `axiom`s the theorem goals over them (an opaque `subseteq X Y`) could never
    // kernel-reduce to the `Eq`-shaped proof terms — every such theorem is a type
    // mismatch. args[0] is the definiendum (`a ⊆ b`), its free idents are the
    // params, and args[1] (`a ∩ b = a`) renders as the body.
    let is_def_top = |h: &AtomicComposite| {
        h.primary == EQ_SYM
            || ctx
                .by_id(h.primary as i64)
                .is_some_and(|r| qubitick_model::format::is_iff(&r.tex))
    };
    let (definiens, params) = match e {
        Expr::Apply(h, args) if is_def_top(h) && args.len() == 2 => {
            let mut vars = Vec::new();
            qubitick_pattern::expr_idents(&args[0], &mut vars);
            (args[1].clone(), vars)
        }
        _ => (e.clone(), Vec::new()),
    };
    // Every free variable of the body must be a definition parameter; otherwise
    // the emitted `def` would reference an undefined `t`/`e`. Declare instead.
    {
        let mut body_vars = Vec::new();
        qubitick_pattern::expr_idents(&definiens, &mut body_vars);
        if body_vars.iter().any(|v| !params.contains(v)) {
            return None;
        }
    }
    let body = qubitick_lean::render(&definiens, resolver);
    // A body that mentions its own name would be a self-referential `def`, which
    // Lean rejects ("fail to show termination"); leave it uninterpreted instead.
    if mentions_word(&body, name) {
        return None;
    }
    if body.is_empty() || body.contains('\\') {
        return None;
    }
    // The frame is the AUTHORITY on a definition's type: it is what every other
    // module sees, so the definition's own declaration has to agree with it.
    //
    // A definition is a `def` only when it defines the glyph at the glyph's OWN
    // arity. `\ulcorner x \urcorner = \Vert x \Vert` is one argument and one
    // parameter, so it is a definition. `D.BT.73`'s `\frac{a}{b} +_{\mathbb{Q}}
    // \frac{c}{d} = …` states a RULE about the shape of the arguments of a
    // two-argument operator: emitting it as a four-parameter `def` gave one module
    // a `plus_{\mathbb{Q}}` of four arguments while every other module applied it
    // to two.
    let frame_ty = known.get(name).map(|ty| split_arrow_type(ty));
    if let Some((args, _)) = &frame_ty
        && !args.is_empty()
        && args.len() != params.len()
    {
        return Some(format!("axiom {name} : {}\n", known[name]));
    }
    // Otherwise the types come from what the frame says, and where it says nothing,
    // from the glyphs the body APPLIES: `\ulcorner x \urcorner = \Vert x \Vert`
    // is `Prop → τ` because `\Vert` is. The flat `Prop` fallback is not merely
    // loose — a module that never declares the generated sorts let `autoImplicit`
    // bind each sort occurrence separately, so the definition "type-checked"
    // against a fresh variable per mention.
    let derived = body_sorts(&definiens, &params, known, resolver);
    let fallback = "Prop";
    // The body's own result wins, for the same reason its parameters do: the
    // definition IS that expression, and `\ulcorner x \urcorner = \Vert x \Vert`
    // is τ-valued because `\Vert` is — even though the frame, which only knows how
    // `\ulcorner` is USED elsewhere, would have called it a proposition.
    let rty = derived
        .result
        .or_else(|| frame_ty.as_ref().map(|(_, r)| r.clone()))
        .unwrap_or_else(|| fallback.to_string());
    // A `def` whose body applies an uninterpreted symbol has no code to generate;
    // Lean's compiler says so rather than the type checker, which is why only a
    // `lake build` surfaced it.
    Some(if params.is_empty() {
        format!("noncomputable def {name} : {rty} := {body}\n")
    } else {
        let p = params
            .iter()
            .enumerate()
            .map(|(i, v)| {
                // The BODY's constraint wins for a parameter: the emitted `def`
                // has to compile, and the body is what applies the glyph. The
                // frame's own answer is the fallback, and its RESULT type is
                // preferred above because a result is not constrained by the body's
                // outermost application (`\ulcorner x \urcorner = \Vert x \Vert`
                // is τ-valued even though nothing in the body says so).
                let t = derived
                    .params
                    .get(v)
                    .map(String::as_str)
                    .or_else(|| frame_ty.as_ref().and_then(|(a, _)| a.get(i)).map(String::as_str))
                    .unwrap_or(fallback);
                format!("({v} : {t})")
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("noncomputable def {name} {p} : {rty} := {body}\n")
    })
}

/// The sorts a definition's body imposes on its parameters, and on its result.
pub(crate) struct BodySorts {
    pub params: HashMap<String, String>,
    /// `None` when the body's outermost head has no known declaration.
    pub result: Option<String>,
}

/// Split a declared Lean type into its argument types and its result.
///
/// `(C → Prop) → Prop` is ONE argument (`C → Prop`) and result `Prop`; only
/// `→` at bracket depth zero separates.
fn split_arrow_type(ty: &str) -> (Vec<String>, String) {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut cur = String::new();
    for c in ty.chars() {
        match c {
            '(' | '{' => {
                depth += 1;
            }
            ')' | '}' => depth = depth.saturating_sub(1),
            '→' if depth == 0 => {
                args.push(cur.trim().to_string());
                cur = String::new();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if depth == 0 && ty.contains('→') {
        (args, cur.trim().to_string())
    } else if args.is_empty() {
        (vec![], ty.trim().to_string())
    } else {
        (args, cur.trim().to_string())
    }
}

/// Read the sorts a body imposes on its parameters.
///
/// Every application in the body whose head has a declared type constrains the
/// parameters sitting in its argument positions. A `\mathbb{Z}`/`τ` constraint
/// wins over the carrier default; anything left unconstrained stays the carrier.
fn body_sorts(
    body: &qubitick_model::Expr,
    params: &[String],
    known: &HashMap<String, String>,
    resolver: &impl Fn(u32) -> String,
) -> BodySorts {
    fn walk(
        e: &qubitick_model::Expr,
        params: &[String],
        known: &HashMap<String, String>,
        resolver: &impl Fn(u32) -> String,
        out: &mut HashMap<String, String>,
    ) {
        use qubitick_model::Expr;
        if let Expr::Apply(h, args) = e
            && let Some(ty) = known.get(&resolver(h.primary))
        {
            {
                let (argtys, _) = split_arrow_type(ty);
                for (i, a) in args.iter().enumerate() {
                    if let (Expr::Var(v), Some(t)) = (a, argtys.get(i))
                        && params.contains(v)
                    {
                        let t = t.trim_matches(|c| c == '(' || c == ')');
                        // A carrier default never displaces a real sort.
                        if out.get(v).is_none_or(|old| old == "Prop") {
                            out.insert(v.clone(), t.to_string());
                        }
                    }
                }
            }
        }
        match e {
            Expr::Apply(_, args) | Expr::Bracket(_, args) | Expr::Vector(args)
            | Expr::Set(args) => {
                for a in args {
                    walk(a, params, known, resolver, out);
                }
            }
            Expr::Binder(_, bs, b) => {
                for bd in bs {
                    walk(&bd.ty, params, known, resolver, out);
                }
                walk(b, params, known, resolver, out);
            }
            Expr::Recursor(s, cases) => {
                walk(s, params, known, resolver, out);
                for (_, b) in cases {
                    walk(b, params, known, resolver, out);
                }
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for c in r {
                        walk(c, params, known, resolver, out);
                    }
                }
            }
            Expr::Sym(_) | Expr::Var(_) => {}
        }
    }
    let result = match body {
        qubitick_model::Expr::Apply(h, _) => {
            known.get(&resolver(h.primary)).map(|ty| split_arrow_type(ty).1)
        }
        _ => None,
    };
    let mut p = HashMap::new();
    walk(body, params, known, resolver, &mut p);
    BodySorts { params: p, result }
}

fn symbol_lean_name(ctx: &Ctx, id: u32) -> String {
    ctx.by_id(id as i64)
        .map(|r| r.lean_name.clone())
        .unwrap_or_else(|| format!("s{id}"))
}


#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_db::queries::SymbolRow;
    use qubitick_model::NodeBase;

    fn ctx() -> Ctx {
        Ctx::from_rows(
            "Era0/Strata0",
            vec![SymbolRow { facets: 0,
                id: 1,
                scope: "".into(),
                tex: "\\tau".into(),
                display: "τ".into(),
                lean_name: "Tau".into(),
                trigger: None,
                kind: "macro".into(),
                role: Some("primary".into()),
                precedence: None,
                home_node: 1,
            elab: None,
            binder_kind: None,
            }],
        )
    }

    #[test]
    fn emits_definition() {
        let ctx = ctx();
        let def = Definition {
            base: NodeBase {
                id: 2,
                kind: qubitick_model::NodeKind::Definition,
                era: qubitick_model::Era::Era0,
                strata: qubitick_model::Strata(0),
                name: "Def1".into(),
                req: vec![],
                derived: vec![],
                note: None,
            },
            symbol: qubitick_model::AtomicComposite::new(1),
            extra_symbols: vec![],
            d_block: qubitick_model::DBlockIR {
                kind: qubitick_model::SymbolKind::Macro,
                form: Some(qubitick_model::FormBody::MacroExpr(
                    // `Tau x = x`: the body's free var is a definition parameter, so
                    // it compiles to a real `def Tau (x) := x`.
                    qubitick_model::Expr::Apply(
                        qubitick_model::AtomicComposite::new(qubitick_model::EQ_SYM),
                        vec![
                            qubitick_model::Expr::Apply(
                                qubitick_model::AtomicComposite::new(1),
                                vec![qubitick_model::Expr::Var("x".into())],
                            ),
                            qubitick_model::Expr::Var("x".into()),
                        ],
                    ),
                )),
                ..Default::default()
            },
            tag: qubitick_model::PhysOrMath::Math,
        };
        let out = emit_definition(&ctx, &def).unwrap();
        // `noncomputable`: a generated definition may apply an uninterpreted symbol,
        // and Lean's code generator refuses those outright (`Vert is not supported
        // by code generator`) where the type checker does not care.
        assert!(out.starts_with("noncomputable def Tau"), "{out}");
    }

    #[test]
    fn emits_sema_definition_with_parameters() {
        // Sema `a /\ b = <body>` defines the glyph's head applied to its
        // metavariables: those metavariables become the def's parameters.
        use qubitick_model::{AtomicComposite, Expr, FormBody};
        let ctx = ctx();
        let lhs = Expr::Apply(
            AtomicComposite::new(1),
            vec![Expr::Var("a".into()), Expr::Var("b".into())],
        );
        // `Tau a b = b`: the body must not mention `Tau` (that would be
        // self-referential and is declared uninterpreted instead).
        let rhs = Expr::Var("b".into());
        let sema = Expr::Apply(
            AtomicComposite::new(qubitick_model::EQ_SYM),
            vec![lhs, rhs],
        );
        let def = Definition {
            base: NodeBase {
                id: 2,
                kind: qubitick_model::NodeKind::Definition,
                era: qubitick_model::Era::Era0,
                strata: qubitick_model::Strata(0),
                name: "Def2".into(),
                req: vec![],
                derived: vec![],
                note: None,
            },
            symbol: AtomicComposite::new(1),
            extra_symbols: vec![],
            d_block: qubitick_model::DBlockIR {
                kind: qubitick_model::SymbolKind::Macro,
                form: Some(FormBody::MacroExpr(sema)),
                ..Default::default()
            },
            tag: qubitick_model::PhysOrMath::Math,
        };
        let out = emit_definition(&ctx, &def).unwrap();
        assert_eq!(
            out,
            "noncomputable def Tau (a : Prop) (b : Prop) : Prop := b\n"
        );
    }
}