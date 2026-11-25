//! Structural hashing (SymbolDB §13).
//!
//! Hashes are 16-byte (`u128`) values computed deterministically from structure,
//! independent of assigned `SymbolId`s — so they are stable across builds where
//! symbol ids may shift. `H_expr` hash-conses the RAM cache by these keys.

use qubitick_model::{AtomicComposite, Binder, BinderKind, Expr, Pattern};
use xxhash_rust::xxh3::Xxh3;

/// Hash a symbol declaration identity: `H(scope || tex || kind)` (SymbolDB §13.1).
///
/// `scope` is the symbol's OWN declaration scope. Including it makes the same tex
/// in different eras hash differently (no cross-era conflation on reconstruction)
/// while remaining stable across build scopes: callers must pass each symbol row's
/// own `scope`, not a build-scope parameter, so write-time and read-time hashes
/// always agree.
pub fn hash_symbol(scope: &str, tex: &str, kind: &str) -> u128 {
    let mut h = Xxh3::new();
    h.update(scope.as_bytes());
    h.update(&[0x00]);
    h.update(tex.as_bytes());
    h.update(&[0x00]);
    h.update(kind.as_bytes());
    h.digest128()
}

/// Hash an atomic composite: `H_symbol(primary) || mod hashes` (SymbolDB §13.2).
pub fn hash_atomic(ac: &AtomicComposite, symbol_hash_of: &dyn Fn(u32) -> u128) -> u128 {
    let mut h = Xxh3::new();
    write_u128(&mut h, symbol_hash_of(ac.primary));
    write_opt_mod(&mut h, &ac.mods.l_sup, &symbol_hash_of);
    write_opt_mod(&mut h, &ac.mods.r_sup, &symbol_hash_of);
    write_opt_mod(&mut h, &ac.mods.l_sub, &symbol_hash_of);
    write_opt_mod(&mut h, &ac.mods.r_sub, &symbol_hash_of);
    h.digest128()
}

fn write_opt_mod(h: &mut Xxh3, m: &Option<Box<AtomicComposite>>, sh: &dyn Fn(u32) -> u128) {
    match m {
        Some(ac) => write_u128(h, hash_atomic(ac, sh)),
        None => h.update(&[0x00]),
    }
}

fn write_u128(h: &mut Xxh3, v: u128) {
    h.update(&v.to_le_bytes());
}

/// Hash an `Expr` (SymbolDB §13.2). `symbol_hash_of` maps a `SymbolId` to its
/// build-independent `H_symbol`.
pub fn hash_expr(e: &Expr, symbol_hash_of: &dyn Fn(u32) -> u128) -> u128 {
    let mut h = Xxh3::new();
    match e {
        Expr::Sym(ac) => {
            h.update(&[0x01]);
            write_u128(&mut h, hash_atomic(ac, symbol_hash_of));
        }
        Expr::Var(v) => {
            h.update(&[0x02]);
            h.update(v.as_bytes());
        }
        Expr::Apply(head, args) => {
            h.update(&[0x03]);
            write_u128(&mut h, hash_atomic(head, symbol_hash_of));
            for a in args {
                write_u128(&mut h, hash_expr(a, symbol_hash_of));
            }
        }
        Expr::Bracket(id, args) => {
            h.update(&[0x04]);
            write_u128(&mut h, symbol_hash_of(*id));
            for a in args {
                write_u128(&mut h, hash_expr(a, symbol_hash_of));
            }
        }
        Expr::Binder(kind, binders, body) => {
            h.update(&[0x05]);
            h.update(&[binder_kind_tag(*kind)]);
            for (v, ty) in flatten_binders(binders) {
                h.update(v.as_bytes());
                write_u128(&mut h, hash_expr(ty, symbol_hash_of));
            }
            write_u128(&mut h, hash_expr(body, symbol_hash_of));
        }
        Expr::Recursor(scrut, cases) => {
            h.update(&[0x06]);
            write_u128(&mut h, hash_expr(scrut, symbol_hash_of));
            for (p, body) in cases {
                write_u128(&mut h, hash_pattern(p, symbol_hash_of));
                write_u128(&mut h, hash_expr(body, symbol_hash_of));
            }
        }
        Expr::Matrix(rows) => {
            h.update(&[0x07]);
            h.update(&(rows.len() as u64).to_le_bytes());
            for r in rows {
                h.update(&(r.len() as u64).to_le_bytes());
                for c in r {
                    write_u128(&mut h, hash_expr(c, symbol_hash_of));
                }
            }
        }
        Expr::Vector(els) => {
            h.update(&[0x08]);
            for e in els {
                write_u128(&mut h, hash_expr(e, symbol_hash_of));
            }
        }
        Expr::Set(els) => {
            h.update(&[0x09]);
            for e in els {
                write_u128(&mut h, hash_expr(e, symbol_hash_of));
            }
        }
    }
    h.digest128()
}

fn binder_kind_tag(k: BinderKind) -> u8 {
    match k {
        BinderKind::Forall => 0,
        BinderKind::Exists => 1,
        BinderKind::Lambda => 2,
        BinderKind::Integral => 3,
        BinderKind::Limit => 4,
    }
}

/// Flatten a binder group (`∀ a b : 𝔈, …`) into `(var, ty)` pairs, each var
/// sharing the group type.
fn flatten_binders(binders: &[Binder]) -> Vec<(&str, &Expr)> {
    binders
        .iter()
        .flat_map(|b| b.vars.iter().map(move |v| (v.as_str(), &b.ty)))
        .collect()
}

/// Hash a pattern (SymbolDB §13.4).
pub fn hash_pattern(p: &Pattern, symbol_hash_of: &dyn Fn(u32) -> u128) -> u128 {
    let mut h = Xxh3::new();
    h.update(&[0x02]);
    write_u128(&mut h, hash_atomic(&p.constructor, symbol_hash_of));
    for s in &p.sub {
        write_u128(&mut h, hash_pattern(s, symbol_hash_of));
    }
    h.digest128()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qubitick_model::AtomicComposite;

    fn sym_hash(id: u32) -> u128 {
        id as u128
    }

    fn sym(id: u32) -> Expr {
        Expr::Sym(AtomicComposite::new(id))
    }

    #[test]
    fn structural_equality_matches_hash() {
        let a = Expr::Apply(AtomicComposite::new(3), vec![sym(1), sym(2)]);
        let b = Expr::Apply(AtomicComposite::new(3), vec![sym(1), sym(2)]);
        assert_eq!(hash_expr(&a, &sym_hash), hash_expr(&b, &sym_hash));
    }

    #[test]
    fn different_structure_hashes_differ() {
        let a = Expr::Apply(AtomicComposite::new(3), vec![sym(1), sym(2)]);
        let b = Expr::Apply(AtomicComposite::new(3), vec![sym(1), sym(9)]);
        assert_ne!(hash_expr(&a, &sym_hash), hash_expr(&b, &sym_hash));
    }

    #[test]
    fn hash_is_build_independent_of_ids() {
        // Different SymbolId mapping but same "declaration identity" → same key.
        let f = |_id: u32| 42_u128;
        let a = sym(1);
        let b = sym(999);
        assert_eq!(hash_expr(&a, &f), hash_expr(&b, &f));
    }
}