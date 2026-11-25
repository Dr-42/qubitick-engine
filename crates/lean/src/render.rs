use qubitick_model::{AtomicComposite, BinderKind, Expr, Mods, EQ_SYM};
/// Render an expression to Lean surface text.
///
/// `lean_name` maps a `SymbolId` to its declaration's Lean name (e.g. `Delta`).
pub fn render(e: &Expr, lean_name: &impl Fn(u32) -> String) -> String {
    render_node(e, lean_name)
}

fn render_node(e: &Expr, lean_name: &impl Fn(u32) -> String) -> String {
    match e {
        Expr::Sym(ac) => render_atomic(ac, lean_name),
        Expr::Var(v) => v.clone(),
        Expr::Apply(head, args) => {
            if head.primary == EQ_SYM {
                // Equality is the target logic's built-in `=`/Eq, not a corpus
                // symbol. Render it as a left-assoc infix operator.
                return match args.as_slice() {
                    [l, r] => format!("Eq {} {}", fmt_arg(l, lean_name), fmt_arg(r, lean_name)),
                    _ => "_".to_string(),
                };
            }
            let head = render_atomic(head, lean_name);
            if args.is_empty() {
                head
            } else if is_arrow_name(&head) && args.len() == 2 {
                // Type arrow: render as Lean infix `->`, group composites.
                let dom = fmt_arg(&args[0], lean_name);
                let cod = fmt_arg(&args[1], lean_name);
                format!("{dom} -> {cod}")
            } else {
                let args = args
                    .iter()
                    .map(|a| fmt_arg(a, lean_name))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{head} {args}")
            }
        }
        Expr::Bracket(id, args) => {
            let name = lean_name(*id);
            let args = args
                .iter()
                .map(|a| fmt_arg(a, lean_name))
                .collect::<Vec<_>>()
                .join(", ");
            if name.is_empty() {
                format!("({args})")
            } else {
                format!("{name} ⟨{args}⟩")
            }
        }
        Expr::Binder(kind, binders, body) => match kind {
            // Exists is not a binder in Lean: it is HL's `Exists` applied to a lambda
            // (Document 5 §7.1). Multiple binder groups nest the lambdas.
            BinderKind::Exists => {
                let mut out = "Exists".to_string();
                for b in binders {
                    for v in &b.vars {
                        let ty = render(&b.ty, lean_name);
                        out.push_str(&format!(" (fun {v} : {ty} => "));
                    }
                }
                out.push_str(&render(body, lean_name));
                out.push_str(&")".repeat(binders.iter().map(|b| b.vars.len()).sum()));
                out
            }
            _ => {
                let mut acc = String::new();
                for b in binders {
                    let ty = render(&b.ty, lean_name);
                    for v in &b.vars {
                        acc.push_str(&binder_prefix(*kind, v, &ty));
                    }
                }
                acc.push_str(&render(body, lean_name));
                acc
            }
        }
        Expr::Recursor(scrut, cases) => {
            let scrut = fmt_arg(scrut, lean_name);
            let mut out = format!("match {scrut} with");
            for (p, body) in cases {
                let pat = p.binders.join(" ");
                if pat.is_empty() {
                    out.push_str(&format!(" | _ => {}", fmt_arg(body, lean_name)));
                } else {
                    out.push_str(&format!(" | {pat} => {}", fmt_arg(body, lean_name)));
                }
            }
            format!("{out} end")
        }
        Expr::Matrix(rows) => {
            let inner = rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|c| render(c, lean_name))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect::<Vec<_>>()
                .join("; ");
            format!("![{inner}]")
        }
        Expr::Vector(els) => {
            let inner = els
                .iter()
                .map(|e| fmt_arg(e, lean_name))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        }
        Expr::Set(els) => {
            let inner = els
                .iter()
                .map(|e| fmt_arg(e, lean_name))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{inner}}}")
        }
    }
}

fn binder_prefix(kind: BinderKind, name: &str, ty: &str) -> String {
    match kind {
        BinderKind::Forall => format!("∀ ({name} : {ty}), "),
        BinderKind::Lambda => format!("fun {name} : {ty} => "),
        // Integral/Limit are not yet in the corpus (Document 5 §7.1, §7.4);
        // keep an ASCII placeholder so the renderer stays total.
        BinderKind::Integral => format!("int {name} : {ty}, "),
        BinderKind::Limit => format!("lim {name} : {ty}, "),
        BinderKind::Exists => String::new(), // handled by the Expr::Binder arm
    }
}

fn fmt_arg(e: &Expr, lean_name: &impl Fn(u32) -> String) -> String {
    let s = render_node(e, lean_name);
    if is_composite(e) {
        format!("({s})")
    } else {
        s
    }
}

fn is_composite(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Apply(_, _)
            | Expr::Bracket(_, _)
            | Expr::Binder(_, _, _)
            | Expr::Recursor(_, _)
            | Expr::Matrix(_)
    )
}

/// Whether a rendered head name denotes the type arrow.
fn is_arrow_name(name: &str) -> bool {
    matches!(name, "to" | "rightarrow" | "longrightarrow" | "arrow")
}
fn render_atomic(ac: &AtomicComposite, lean_name: &impl Fn(u32) -> String) -> String {
    let base = lean_name(ac.primary);
    let mods = render_mods(&ac.mods, lean_name);
    if mods.is_empty() {
        base
    } else {
        format!("{base}{mods}")
    }
}

fn render_mods(m: &Mods, lean_name: &impl Fn(u32) -> String) -> String {
    let parts: Vec<String> = [&m.l_sup, &m.r_sup, &m.l_sub, &m.r_sub]
        .into_iter()
        .flatten()
        .map(|ac| render_atomic(ac, lean_name))
        .collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(id: u32) -> String {
        match id {
            1 => "Delta".into(),
            2 => "Cap".into(),
            _ => format!("s{id}"),
        }
    }

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn renders_sym() {
        let e = sym(1);
        assert_eq!(render(&e, &name), "Delta");
    }

    #[test]
    fn renders_applications() {
        let e = Expr::Apply(AtomicComposite::new(1), vec![sym(2), Expr::Var("x".into())]);
        assert_eq!(render(&e, &name), "Delta Cap x");
    }

    #[test]
    fn renders_lambda() {
        let e = Expr::Binder(
            BinderKind::Lambda,
            vec![qubitick_model::Binder {
                vars: vec!["x".into()],
                ty: sym(1),
            }],
            Box::new(Expr::Var("x".into())),
        );
        assert_eq!(render(&e, &name), "fun x : Delta => x");
    }

    #[test]
    fn renders_exists_as_lambda_application() {
        // Document 5 §7.1: Exists is HL's `Exists` applied to a lambda, not a binder.
        let e = Expr::Binder(
            BinderKind::Exists,
            vec![qubitick_model::Binder {
                vars: vec!["x".into()],
                ty: sym(1),
            }],
            Box::new(Expr::Var("x".into())),
        );
        assert_eq!(render(&e, &name), "Exists (fun x : Delta => x)");
    }

    #[test]
    fn renders_forall_with_binder_parens() {
        // Document 5 §7.1: Forall uses parenthesized per-variable binders.
        let e = Expr::Binder(
            BinderKind::Forall,
            vec![qubitick_model::Binder {
                vars: vec!["a".into()],
                ty: sym(1),
            }],
            Box::new(Expr::Var("a".into())),
        );
        assert_eq!(render(&e, &name), "∀ (a : Delta), a");
    }

    #[test]
    fn renders_type_arrow_infix_right_assoc() {
        // `\to` head applied to two operands renders as a Lean infix `->`.
        let e = Expr::Apply(
            AtomicComposite::new(9), // arrow symbol (lean name `to`)
            vec![sym(2), Expr::Apply(AtomicComposite::new(9), vec![sym(2), sym(1)])],
        );
        // name(9) must be accepted as an arrow name. 
        let names = |id: u32| match id {
            9 => "to".into(),
            1 => "Delta".into(),
            2 => "Cap".into(),
            _ => format!("s{id}"),
        };
        assert_eq!(render(&e, &names), "Cap -> (Cap -> Delta)");
    }

    #[test]
    fn renders_equality_as_infix_eq() {
        let names = |id: u32| match id {
            1 => "Delta".into(),
            _ => format!("s{id}"),
        };
        let e = Expr::Apply(
            AtomicComposite::new(EQ_SYM),
            vec![Expr::Var("a".into()), Expr::Var("b".into())],
        );
        assert_eq!(render(&e, &names), "Eq a b");
    }

    #[test]
    fn renders_matrix() {
        let e = Expr::Matrix(vec![
            vec![sym(1), sym(2)],
            vec![sym(2), sym(1)],
        ]);
        assert_eq!(render(&e, &name), "![Delta, Cap; Cap, Delta]");
    }
    #[test]
    fn renders_vector() {
        let e = Expr::Vector(vec![sym(1), sym(2), Expr::Var("x".into())]);
        assert_eq!(render(&e, &name), "[Delta, Cap, x]");
    }
    #[test]
    fn renders_set() {
        let e = Expr::Set(vec![sym(1), sym(2)]);
        assert_eq!(render(&e, &name), "{Delta, Cap}");
    }
    #[test]
    fn renders_bracket() {
        let e = Expr::Bracket(2, vec![sym(1)]);
        assert_eq!(render(&e, &name), "Cap ⟨Delta⟩");
    }
}