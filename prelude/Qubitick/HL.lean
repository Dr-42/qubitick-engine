prelude
import Qubitick.Core

-- ═══════════════════════════════════════════════════════════════════
-- Qubitick.HL — the high-level surface ground (directive #29).
--
-- "The Core should depend on AND, XOR, the nullity equality and tick as
--  the → mechanism. That is it. No and or, etc etc. Rest all is HL surface."
--
-- This file is the Lean-side PLUMBING of the HL layer: the Prop toolkit
-- (False/True/Not/And/Or/Exists/Iff) at ROOT names — the bare-prelude root
-- is empty, so these names are free, and artifacts reference them bare
-- (Not (Eq …), Exists (fun p : Vtx => …), Px motives over False/True).
-- Moved VERBATIM out of the pre-reduction Core.lean: same inductives and
-- structures, same root-marker discipline (lcErased/lcAny arrive via
-- import Qubitick.Core), zero safe axioms.
--
-- Layering doctrine (dir#29 + canon symbol table):
--   Core  = Eq machinery only (nullity equality; kernel-native; cite
--           license) — the four mechanisms ∩ Δ ∅ τ are corpus A1 content.
--   HL    = everything else. Two kinds:
--     (a) THIS file — Prop-level toolkit plumbing (Lean connective shapes
--         that statements need: ¬∃τ⁻¹, ∃-registration, Iff in the union
--         form). Recursors (False.rec, Or.rec, Exists.intro) stay available
--         to the emitter's discrimination/registration machinery.
--     (b) CORPUS defs — the event-level connectives on 𝔈, DB-driven and
--         audited: ¬ a = 𝟙 Δ a (D.BT.1), a ∧ b = a ∩ b (D.BT.2),
--         a ∨ b = 𝟙 Δ ((𝟙 Δ a) ∩ (𝟙 Δ b)) (D.BT.3), X ⟹ Y = 𝟙 Δ (X ∩ (𝟙 Δ Y))
--         (D.BT.4), a ⟺ b = 𝟙 Δ (a Δ b) (D.BT.5 — "equivalence is the
--         nullity of the difference"). Canon declares ¬ ∧ ⟹ ⟺ at A3.
--
-- Name-collision law: root names here must never clash with corpus root
-- names (neg/andm/orm/impl/iffm/iffp/subset/mem/Px/Tick-namespace/…).
-- Audit: build_audit_manifest scans this file like any emitted module —
-- its 12 decls (7 Prop toolkit + 5 P3) carry the same #print-axioms
-- footprint gate as the corpus.
-- ═══════════════════════════════════════════════════════════════════

inductive False : Prop
inductive True : Prop
  | intro : True

def Not (a : Prop) : Prop := a → False

structure And (a b : Prop) : Prop where
  intro ::
  left : a
  right : b

inductive Or (a b : Prop) : Prop
  | inl : a → Or a b
  | inr : b → Or a b

inductive Exists {α : Sort u} (p : α → Prop) : Prop
  | intro (w : α) (h : p w) : Exists p

structure Iff (a b : Prop) : Prop where
  intro ::
  mp  : a → b
  mpr : b → a

-- ─── P3 additions (GROUND pre-test §2.2; wires 1793741512101 +
-- 1793742973172) — the five vendored-form prerequisites. The A4 ladder's
-- frozen rec-term bodies (project/code/Qubitick/Era0/Strata1/A4_Build.lean)
-- need exactly this much Init-form machinery beyond the Prop toolkit:
--   Bool + Decidable (classic inductive, isFalse-first — generator's
--   recommendation ACCEPTED; the emitted corpus never synthesizes
--   Decidable — no if/decide under prelude — so the classic form is
--   axiom-neutral), class DecidableEq (instances via explicit .mk in the
--   vendored tree), and the False-elimination pair as noncomputable DEFS
--   (not theorems — codegen boundary; kernel iota is
--   noncomputable-agnostic, so the all-rfl demo pattern survives).
-- Placement per dir#29: Core = the Eq machinery only — everything else
-- is HL surface. Bodies byte-faithful to the generator's §0 staging;
-- the single mechanical change is `Qubitick.Core.Not` → bare `Not`
-- (root name, same file). No genInjectivity guard needed: neither
-- inductive has a recursive constructor argument (the Tick precedent).
-- Name-collision law check: Bool/Decidable/DecidableEq/absurd/False.elim
-- clash with no corpus root name (DB stack audited at S2 landing).

inductive Bool where
  | false : Bool
  | true : Bool

inductive Decidable (p : Prop) where
  | isFalse (h : Not p) : Decidable p
  | isTrue (h : p) : Decidable p

class DecidableEq (α : Sort u) where
  dec : (a b : α) → Decidable (Eq a b)

noncomputable def False.elim {C : Sort u} (h : False) : C :=
  False.rec (motive := fun _ => C) h
noncomputable def absurd {a : Prop} {b : Sort v} (h₁ : a) (h₂ : Not a) : b :=
  False.elim (h₂ h₁)
