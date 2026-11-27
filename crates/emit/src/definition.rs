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
        &std::cell::Cell::new(false),
    )
}

/// As [`emit_definition`], but when `is_sort` the glyph is used as a binder sort
/// elsewhere in the module, so it is declared as a `Type`-valued space.
#[allow(clippy::too_many_arguments)]
pub fn emit_definition_in(
    ctx: &Ctx,
    def: &Definition,
    is_sort: bool,
    _arity: usize,
    fn_names: &std::collections::HashMap<u32, String>,
    sorts: &std::collections::HashSet<u32>,
    formations: &HashMap<u32, (String, Vec<u32>)>,
    known: &HashMap<String, String>,
    // SET WHEN THE BODY REACHES A FIELD OF THE INSTANCE (`R.plus`). A caller decides where
    // the definition belongs by asking this, not by searching the rendered text for "R.":
    // the answer is a property of the SYMBOL (a `bound` field), and a string test would
    // trip over any corpus that writes an expression containing those two characters.
    needs_instance: &std::cell::Cell<bool>,
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
        /* A GLYPH REACHED THROUGH THE INSTANCE IS NAMED THROUGH IT. `plus` is a field of a
           class the GROUND declares, and a definition is written outside that class, so its
           body says `R.plus` and needs the instance in scope. Stripping the qualifier here
           is what made an era ground read `:= plus a a` with no `plus` in scope — the bug
           that stopped `lake --mesh` compiling. */
        if name.starts_with("R.") {
            needs_instance.set(true);
        }
        name
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
        // The frame declares a sort glyph (`def X : Type := Prop`); a second declaration
        // here collided with it, and an `axiom` was the wrong shape for it anyway.
        return Ok(String::new());
    }
    match &def.d_block.form {
        Some(qubitick_model::FormBody::MacroExpr(e)) => {
            // A compilable Sema becomes a real `def` (so proofs can unfold it
            // definitionally); otherwise the glyph is declared uninterpreted. An
            // operator defined by recursion over a declared formation is emitted as a
            // recursor `def` first; only if that is not applicable do we fall back to
            // a term.
            /* A PARTIAL DEFINITION IS A HOLE, AND THE HOLE IS NAMED.
             *
             * `\tau_0 \oplus b = b` defines one case of an operator over a carrier with two
             * constructors. The case machinery refuses it (it would be a partial match), and
             * the fallback read the equation as a TERM instead — one parameter against the
             * frame's two — and emitted `axiom oplus : tau → tau → tau`: an assumption nobody
             * wrote, in place of the definition the corpus wrote, while the build reported the
             * file verified. The constructors the cases miss are computable, so they are said. */
            /* A CONTROL WORD WHERE AN OPERAND GOES IS NOT A METAVARIABLE.
             *
             * `\tau_0 \in \frobnicate = b` reads: the operator, the constructor, and then
             * `\frobnicate` — a glyph the corpus never declares, written where a variable
             * name goes. Nothing can be rendered for it, and the sentence the build used to
             * give (`cannot be rendered as a Lean term ... simplify the body`) named neither
             * the glyph nor the file's mistake. It was also masked: the case machinery spoke
             * first, about a missing constructor, because one case really was missing. */
            if let Some(word) = undeclared_word(e) {
                return Err(anyhow::anyhow!(
                    "the `Sema:` body of `{name}` names `{word}`, which no node declares.\n  \
                     A glyph is declared by a node whose glyph line writes it (`a {word} b` \
                     makes it infix); a metavariable is a plain name (`x`, `b`), never a \
                     control word."
                ));
            }
            if let Some(said) = missing_cases(ctx, e, formations) {
                return Err(anyhow::anyhow!("{said}"));
            }
            match formation_match_def_text(ctx, &name, e, &resolver, formations, known)
                .or_else(|| recursor_def_text_formation(ctx, &name, e, &resolver, formations))
                .or_else(|| macro_def_text(ctx, &name, e, &resolver, known))
            {
                Some(text) => Ok(text),
                None => Err(anyhow::anyhow!(
                    "`Sema:` body for `{name}` cannot be rendered as a Lean term.\n  \
                     Bind the glyph with `Type:` to a ground name, or simplify the body."
                )),
            }
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
        /* A BODY THE ENGINE CANNOT COMPILE IS A BUILD ERROR, NOT AN ASSUMPTION.
         *
         * This used to emit `axiom {name} : <type>` — an uninterpreted symbol nobody
         * chose, in place of the definition the corpus wrote. A `Sema:` that does not
         * render as a term is a hole in the corpus: bind the glyph with `Type:` to a
         * ground name, or write a body that compiles. */
        Some(_) => Err(anyhow::anyhow!(
            "`Sema:` body for `{name}` cannot be rendered as a Lean term.\n  \
             Bind the glyph with `Type:` to a ground name, or simplify the body."
        )),
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
            // A glyph the corpus declares with a `Type:` and defines nowhere: the
            // engine writes NO declaration for it. Either the ground carries the name
            // (`Type: <name>` — the binding is the home) or the corpus has a hole, and
            // the frame says which (crates/emit/src/module.rs, `Frame::needed`).
            let _ = sig;
            Ok(String::new())
        }
    }
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

/// A `Sema:` WHOSE SECTIONS ARE THE CASES OF A DEFINITION OVER A DECLARED CARRIER.
///
/// ```text
/// a \oplus b
/// Sema:
/// (\tau_0 \oplus b = b) \land ((^+ a) \oplus b = ^+ (a \oplus b))
/// ```
///
/// Each section's definiendum applies the operator to a CONSTRUCTOR and to the operator's
/// other parameters; the section's right side is that case's value. That is the whole
/// definition, and it is compiled to the carrier's own recursor:
///
/// ```text
/// noncomputable def oplus : tau → tau → tau :=
///   fun __x => tau.rec (motive := fun _ => tau → tau)
///     (fun b => b)
///     (fun a ih => fun b => caret (ih b))
///     __x
/// ```
///
/// WHY THE RECURSOR AND NOT `match`: Lean's equation compiler encodes a multi-argument
/// pattern match through `PProd`, and a module emitted under `prelude` has no `PProd` —
/// `Unknown constant PProd.mk`, measured on exactly this definition. The recursor is
/// spelled out instead, which also makes the case order and the motive explicit.
///
/// Two shapes are refused rather than guessed: a section whose pattern is not a constructor
/// of one declared formation, and sections that do not cover every constructor. A partial
/// match is a hole in the corpus, and the alternative (an `axiom` for the operator) is an
/// assumption nobody wrote.
pub(crate) fn formation_match_def_text(
    ctx: &Ctx,
    name: &str,
    e: &qubitick_model::Expr,
    resolver: &impl Fn(u32) -> String,
    formations: &HashMap<u32, (String, Vec<u32>)>,
    known: &HashMap<String, String>,
) -> Option<String> {
    use qubitick_model::Expr;
    if formations.is_empty() {
        return None;
    }

    // The sections: the body itself when it is one equation, or the conjuncts of a chain.
    let mut sections: Vec<&Expr> = Vec::new();
    fn split<'a>(ctx: &Ctx, x: &'a Expr, out: &mut Vec<&'a Expr>) {
        match x {
            Expr::Apply(h, args)
                if args.len() == 2 && crate::module::is_conjunct_head(ctx, h.primary) =>
            {
                split(ctx, &args[0], out);
                split(ctx, &args[1], out);
            }
            other => out.push(other),
        }
    }
    split(ctx, e, &mut sections);

    /* One arm per section: the constructor it matches, the variables that pattern binds,
       and the value. The variables are what the recursive case adds to the recursor's
       argument list, and what a recursive call inside the value mentions. */
    struct Arm {
        ctor: u32,
        vars: Vec<String>,
        rhs: String,
    }
    let mut carrier: Option<u32> = None;
    let mut arity: Option<usize> = None;
    let mut params: Option<Vec<String>> = None;
    let mut arms: Vec<Arm> = Vec::new();
    /* WHICH ARGUMENT THE CASES MATCH ON IS READ, NOT ASSUMED.
     *
     * A recursor matches one argument; the sections vary that one and hold the others fixed.
     * Requiring it to be the FIRST typed `a \star b` (an `\iota` by a `\tau`) as a `\tau → \tau`
     * operator, because the second argument's pattern was a `\tau_0` where a variable was
     * expected. The position is the one whose patterns are constructors of one formation; every
     * other position must be a variable, since the recursor holds it fixed. */
    let mut match_at: Option<usize> = None;
    for section in &sections {
        let Expr::Apply(h, sides) = section else { return None };
        if h.primary != qubitick_model::EQ_SYM || sides.len() != 2 {
            return None;
        }
        let Expr::Apply(head, args) = &sides[0] else { return None };
        if resolver(head.primary) != name {
            return None;
        }
        // The position whose term is a CONSTRUCTOR of one declared formation is the one the
        // cases match on. Every other position must be a variable: a recursor holds it fixed.
        let position = args.iter().position(|a| match a {
            Expr::Sym(ac) => formations.values().any(|(_, cs)| cs.contains(&ac.primary)),
            Expr::Apply(h2, _) => formations.values().any(|(_, cs)| cs.contains(&h2.primary)),
            _ => false,
        })?;
        match match_at {
            Some(p) if p != position => return None,
            _ => match_at = Some(position),
        }
        // Every arm threads the SAME parameter names through — they are one def's arguments.
        let mut names: Vec<String> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if i == position {
                continue;
            }
            let Expr::Var(v) = a else { return None };
            names.push(v.clone());
        }
        match &params {
            Some(p) if *p != names => return None,
            _ => params = Some(names),
        }
        let first = &args[position];
        let (owner, _) = formations.iter().find(|(_, (_, cs))| match first {
            Expr::Sym(ac) => cs.contains(&ac.primary),
            Expr::Apply(h2, _) => cs.contains(&h2.primary),
            _ => false,
        })?;
        match carrier {
            Some(c) if c != *owner => return None,
            _ => carrier = Some(*owner),
        }
        match arity {
            Some(a) if a != args.len() => return None,
            _ => arity = Some(args.len()),
        }
        let (ctor, vars) = match first {
            Expr::Sym(ac) => (ac.primary, Vec::new()),
            Expr::Apply(h2, sub) => {
                let mut vars = Vec::new();
                for a in sub {
                    let Expr::Var(v) = a else { return None };
                    vars.push(v.clone());
                }
                (h2.primary, vars)
            }
            _ => return None,
        };
        let rhs = qubitick_lean::render(&sides[1], resolver);
        if rhs.is_empty() || rhs.contains('\\') {
            return None;
        }
        arms.push(Arm { ctor, vars, rhs });
    }
    let carrier_id = carrier?;
    let (carrier_name, order) = formations.get(&carrier_id)?.clone();
    let n = arity?;
    // Every constructor of the formation exactly once, in the formation's own order.
    if arms.len() != order.len() || !order.iter().all(|c| arms.iter().any(|a| a.ctor == *c)) {
        return None;
    }
    let mut ordered: Vec<&Arm> = Vec::new();
    for c in &order {
        ordered.push(arms.iter().find(|a| a.ctor == *c)?);
    }

    if std::env::var("QDBG").is_ok() {
        eprintln!("[def] name={name} known={:?} arity={n}", known.get(name));
    }
    // The type is the frame's — the Sema composed it (`module::types_from_semas`).
    let ty = known.get(name).cloned().unwrap_or_else(|| {
        let mut t = String::new();
        for _ in 0..n {
            t.push_str(&carrier_name);
            t.push_str(" → ");
        }
        t.push_str(&carrier_name);
        t
    });
    /* THE MOTIVE IS THE TYPE'S, NOT THE CARRIER'S.
     *
     * A recursor holds every argument but the matched one fixed, so the motive is written over
     * the arguments AFTER it and then the result — and for an operator over one carrier those
     * are all that carrier, which is why writing it out by name worked. A MIXED operator
     * (`a \star b` applied to a `\tau` and an `\iota`, returning `\iota`) has the composed
     * type `tau → iota → iota` and the motive `fun _ => iota → iota`; written over the
     * matched argument's carrier it asked Lean for `tau → tau` and every case body the corpus
     * wrote was a type mismatch. The composed type is the answer already; it is split. */
    let (arg_tys, res_ty) = split_arrow_type(&ty);
    let typed = arg_tys.len() == n;
    let pos = match_at?;
    let all: Vec<String> = params.clone().unwrap_or_default();
    // The arguments BEFORE the matched one stay as outer lambdas; those AFTER it ride in the
    // motive and in every case, because the recursor holds them fixed.
    let pre: Vec<String> = all.iter().take(pos).cloned().collect();
    let post: Vec<String> = all.iter().skip(pos).cloned().collect();
    let arrow_to = |args: &[String]| -> String {
        let mut t = String::new();
        for _ in args {
            t.push_str(&carrier_name);
            t.push_str(" → ");
        }
        t.push_str(&carrier_name);
        t
    };
    let _ = arrow_to(&all);
    let mut head = String::new();
    for v in &pre {
        head.push_str(v);
        head.push(' ');
    }
    let mut out = format!("noncomputable def {name} : {ty} :=\n  fun {head}__x => {carrier_name}.rec\n");
    // The motive: the arguments after the matched one, then the result.
    let mut motive = String::from("fun _ => ");
    for i in 0..post.len() {
        // `params` holds every argument but the matched one, so the matched position sits
        // between `pre` and `post` in the type's own list.
        let t = if typed {
            arg_tys[pos + 1 + i].as_str()
        } else {
            carrier_name.as_str()
        };
        motive.push_str(t);
        motive.push_str(" → ");
    }
    motive.push_str(if typed { res_ty.as_str() } else { carrier_name.as_str() });
    out.push_str(&format!("    (motive := {motive})\n"));
    for arm in &ordered {
        /* THE RECURSOR'S CASE: its constructor's arguments, then the induction hypothesis;
           the arguments the recursor holds fixed follow, because the value is written in
           them. */
        let mut case = String::new();
        if !arm.vars.is_empty() {
            case.push_str(&format!("fun {} ih => ", arm.vars.join(" ")));
        }
        if !post.is_empty() {
            case.push_str(&format!("fun {} => ", post.join(" ")));
        }
        /* A RECURSIVE CALL IN THE VALUE IS THE HYPOTHESIS. The call the corpus writes names
           every argument (`a \star b`); the recursor already holds the result for the
           matched one, so `star a b` becomes `ih` applied to the fixed arguments. */
        let mut value = arm.rhs.clone();
        if let Some(rec) = arm.vars.first() {
            let mut call_args: Vec<String> = pre.clone();
            call_args.push(rec.clone());
            call_args.extend(post.iter().cloned());
            let call = format!("{name} {}", call_args.join(" "));
            let replacement = if post.is_empty() {
                "ih".to_string()
            } else {
                format!("ih {}", post.join(" "))
            };
            value = value.replace(&call, &replacement);
        }
        case.push_str(&value);
        out.push_str(&format!("    ({case})\n"));
    }
    out.push_str("    __x\n");
    Some(out)
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

/// The constructors a case definition does not cover, as a sentence — or `None` when the
/// body is not a set of constructor equations over one declared formation.
///
/// A partial case definition must not fall through to the term path: the term path sees one
/// parameter where the operator has two and writes an `axiom`, which is an assumption in place
/// of the definition the corpus wrote.
pub(crate) fn missing_cases(
    ctx: &Ctx,
    e: &qubitick_model::Expr,
    formations: &HashMap<u32, (String, Vec<u32>)>,
) -> Option<String> {
    use qubitick_model::Expr;
    if formations.is_empty() {
        return None;
    }
    let mut sections: Vec<&Expr> = Vec::new();
    fn split<'a>(ctx: &Ctx, x: &'a Expr, out: &mut Vec<&'a Expr>) {
        match x {
            Expr::Apply(h, args)
                if args.len() == 2 && crate::module::is_conjunct_head(ctx, h.primary) =>
            {
                split(ctx, &args[0], out);
                split(ctx, &args[1], out);
            }
            other => out.push(other),
        }
    }
    split(ctx, e, &mut sections);
    // The formation whose constructors the cases match, and the ones the cases cover.
    let mut seen: Option<(String, Vec<u32>)> = None;
    let mut covered: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for section in &sections {
        let Expr::Apply(h, sides) = section else { return None };
        if h.primary != qubitick_model::EQ_SYM || sides.len() != 2 {
            return None;
        }
        let Expr::Apply(_, args) = &sides[0] else { return None };
        let mut matched = false;
        for a in args {
            let head = match a {
                Expr::Sym(ac) => Some(ac.primary),
                Expr::Apply(h2, _) => Some(h2.primary),
                _ => None,
            };
            let Some(head) = head else { continue };
            if let Some((carrier, order)) = formations
                .values()
                .find(|(_, cs)| cs.contains(&head))
                .map(|(n, cs)| (n.clone(), cs.clone()))
            {
                seen.get_or_insert((carrier, order));
                covered.insert(head);
                matched = true;
            }
        }
        if !matched {
            return None;
        }
    }
    let (carrier, order) = seen?;
    let missing: Vec<String> = order
        .iter()
        .filter(|c| !covered.contains(c))
        .map(|c| format!("`{}`", symbol_lean_name(ctx, *c)))
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(format!(
        "the `Sema:` cases of `{carrier}` are incomplete: no case for {}.\n  \
         An operator defined by cases needs one case per constructor of its carrier — a \
         partial definition is a hole in the corpus, and reading it as a term would make it an \
         assumption nobody wrote.",
        missing.join(", ")
    ))
}


/// The first control word written where a metavariable goes, if the body has one.
///
/// A control word is a glyph: it is declared by a node, or it is nothing. A metavariable is a
/// plain name. A body that writes `\frobnicate` in an operand position asks the engine to
/// render a symbol nobody declared, and it can render nothing.
fn undeclared_word(e: &qubitick_model::Expr) -> Option<String> {
    use qubitick_model::Expr;
    fn walk(e: &Expr, out: &mut Option<String>) {
        if out.is_some() {
            return;
        }
        match e {
            Expr::Var(v) if v.trim_start().starts_with('\\') => {
                // The FIRST control word of the text, without its trailing argument text.
                let t = v.trim();
                let word: String = t
                    .chars()
                    .take_while(|c| *c != ' ' && *c != '{')
                    .collect();
                *out = Some(word);
            }
            Expr::Apply(_, args) | Expr::Bracket(_, args) | Expr::Vector(args) | Expr::Set(args) => {
                for a in args {
                    walk(a, out);
                }
            }
            Expr::Binder(_, binders, body) => {
                for b in binders {
                    walk(&b.ty, out);
                }
                walk(body, out);
            }
            Expr::Recursor(s, cases) => {
                walk(s, out);
                for (_, b) in cases {
                    walk(b, out);
                }
            }
            Expr::Matrix(rows) => {
                for r in rows {
                    for c in r {
                        walk(c, out);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = None;
    walk(e, &mut out);
    out
}
