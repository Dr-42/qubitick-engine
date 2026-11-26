//! `qubitick-emit` — produces `.lean` modules from `Node`s (L5).
//!
//! Depends on `model`, `db`, `ctx`, `lean`, `rewrite`.

pub mod definition;
pub mod imports;
pub mod module;
pub mod theorem;

pub use module::{
    Ground, Options, emit_ground, emit_ground_chain, emit_lean_text, emit_lean_text_indexed,
    emit_lean_text_era, emit_lean_text_meshed, emit_lean_text_split, emit_lean_text_split_with, emit_module,
    lemma_name,
};

/// Resolve a symbol id to its declared Lean name (falls back to `s{id}`).
pub fn lean_name_of(ctx: &qubitick_ctx::Ctx, id: u32) -> String {
    ctx.by_id(id as i64)
        .map(|r| r.lean_name.clone())
        .unwrap_or_else(|| format!("s{id}"))
}

/// Resolve a Lean name, preferring the polysymbol function-variant map
/// (`X` → `X_fn` for applied uses) over the context symbol table.
pub fn resolver<'a>(
    ctx: &'a qubitick_ctx::Ctx,
    fn_names: &'a std::collections::HashMap<u32, String>,
) -> impl Fn(u32) -> String + 'a {
    move |id| fn_names.get(&id).cloned().unwrap_or_else(|| lean_name_of(ctx, id))
}
/// Resolve a symbol id to its emitted Lean name, sending every glyph the corpus uses
/// as a SORT to `Prop`.
///
/// A `Type:`-declared space (`\mathfrak{E}` the carrier, `\tau` the tick sequence)
/// is a space OVER the carrier, and the corpus identifies events with propositions
/// (`Δ` = XOR, `∩` = AND, `1` = True, `∅` = False). A1–A6 carry no `Type:` of their
/// own and are written against `Prop`, so naming the spaces gives Lean a second type
/// that is only equal up to reducibility — and `Δ : Prop → Prop → Prop` then refuses
/// an argument of type `\mathfrak{E}` (`Application type mismatch`). Collapsing every
/// sort glyph to `Prop` keeps a single carrier, which is what the corpus is written
/// against.
pub fn resolver_with_sorts<'a>(
    ctx: &'a qubitick_ctx::Ctx,
    fn_names: &'a std::collections::HashMap<u32, String>,
    sorts: &'a std::collections::HashSet<u32>,
) -> impl Fn(u32) -> String + 'a {
    move |id| {
        /* A GLYPH THE GROUND NAMES IS THE GROUND'S, CARRIER INCLUDED.
         *
         * Collapsing a sort glyph to `Prop` is for a corpus that declares a carrier and
         * leaves it uninterpreted: the engine then has nothing better than `Prop`, and one
         * carrier for the whole corpus keeps the corpus's own arithmetic typechecking.
         * A `Type: E` glyph says otherwise — the ground says what `E` IS — so its name is
         * rendered as written. Collapsing it made a scaffolded ground read
         * `plus : (Prop) → (Prop) → Prop` for a corpus whose carrier is its own `Type`,
         * which is proof-irrelevant and not what the corpus says.
         */
        if let Some(row) = ctx.by_id(id as i64)
            && row.kind == "bound"
        {
            return fn_names.get(&id).cloned().unwrap_or_else(|| row.lean_name.clone());
        }
        // Only the CARRIER is collapsed to `Prop`. A GENERATED sort (`\tau`) is its
        // own inductive type, so collapsing it would make the quantification in a
        // `\mu` theorem unrelated to the cases the corpus proves.
        if sorts.contains(&id) {
            let _name = fn_names.get(&id).cloned().unwrap_or_else(|| lean_name_of(ctx, id));
            // A real inductive carrier — the generated τ/ℤ sorts OR a declarative
            // FORMATION — is NOT collapsed to `Prop`, so a binder over it quantifies
            // over the actual inductive (and a recursor `def` over it reduces).
            let is_formation = ctx
                .by_id(id as i64)
                .map(|r| r.kind == "formation")
                .unwrap_or(false);
            if !is_formation {
                return "Prop".to_string();
            }
        }
        fn_names.get(&id).cloned().unwrap_or_else(|| lean_name_of(ctx, id))
    }
}
