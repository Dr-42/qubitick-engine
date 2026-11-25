prelude

-- ═══════════════════════════════════════════════════════════════════
-- Qubitick.HL — THE GROUND: the one file a project's Lean needs before
-- anything the engine emits can compile.
--
-- It is `prelude` and nothing else: no Init, no Std, no Mathlib. The root
-- namespace a bare prelude starts with is empty, which is what lets these
-- declarations take the ROOT names the emitter references (`Eq`, `rfl`,
-- `Not`, `And`, `Exists`, `lcErased`, `lcAny`, ...). Import a library that
-- already owns those names instead and every declaration below collides.
--
-- What it satisfies is the emitter's contract, stated once in
-- crates/build/src/prelude_contract.rs::REQUIRED — 16 names in three tiers:
--
--   kernel   Eq, Eq.refl, Eq.symm, Eq.trans
--   logic    False, True, Not, And, Or, Exists, Iff, Bool, Decidable, DecidableEq
--   codegen  lcErased, lcAny
--
-- ZERO SAFE AXIOMS: no propext, no funext, no Classical.choice, no
-- Quot.sound. Only `unsafe axiom` codegen markers, at ROOT, exactly as
-- Init/Prelude.lean declares them. `qubitick print-axioms` reports the
-- footprint per theorem; this ground's is empty.
--
-- NO CORPUS VOCABULARY. Nothing here is a mathematical operator: no `\Delta`,
-- no `\cap`, no `\varnothing`, no `\times`. A project declares the glyphs its
-- rows use, or imports a library that has them. This file carries only what
-- Lean needs to read `Eq` and the connectives.
--
-- ONE FILE, DELIBERATELY. A project's own ground replaces `Qubitick/HL.lean`
-- when the engine assembles a ground for checking (crates/build/src/ground.rs
-- ::assemble), so a single file makes that replacement exact — with two, a
-- project shipping only `HL.lean` would silently keep the engine's other half.
--
-- A project that wants Init or Mathlib does not extend this file: it imports
-- the library and declares none of the names below, because those names are
-- already there (measured: `prelude` + `import Init` satisfies the contract
-- with nothing declared).
-- ═══════════════════════════════════════════════════════════════════

-- ── the codegen tier: the two `unsafe axiom` markers, at ROOT ───────
-- Codegen markers at ROOT, exactly as Init/Prelude.lean declares them
-- (parameterized inductives + structures + classes resolve them at root;
--  namespaced placement fails — probed 2026-11-03). `unsafe axiom : Type`
--  only: audit policy (directive #22) permits exactly this shape.
/-- Marker for information that has been erased by the code generator. -/
unsafe axiom lcErased : Type
/-- Marker for type dependency that has been erased by the code generator. -/
unsafe axiom lcAny : Type

-- ─── root Eq: kernel-recognized equality — the nullity-equality machinery
--     (∅-anchoring lands as A1 corpus rows; the cite license is Eq.refl) ───
inductive Eq {α : Sort u} : α → α → Prop
  | refl (a : α) : Eq a a

theorem rfl {α : Sort u} {a : α} : Eq a a := Eq.refl a

theorem Eq.symm {α : Sort u} {a b : α} (h : Eq a b) : Eq b a :=
  Eq.rec (motive := fun x _ => Eq x a) (Eq.refl a) h

theorem Eq.trans {α : Sort u} {a b c : α} (h₁ : Eq a b) (h₂ : Eq b c) : Eq a c :=
  Eq.rec (motive := fun x _ => Eq a x) h₁ h₂

theorem Eq.subst {α : Sort u} {motive : α → Prop} {a b : α} (h₁ : Eq a b) (h₂ : motive a) : motive b :=
  Eq.rec (motive := fun x _ => motive x) h₂ h₁

theorem congrArg {α : Sort u} {β : Sort v} {a₁ a₂ : α} (f : α → β) (h : Eq a₁ a₂) : Eq (f a₁) (f a₂) :=
  Eq.rec (motive := fun x _ => Eq (f a₁) (f x)) (Eq.refl (f a₁)) h

theorem congr {α : Sort u} {β : Sort v} {f₁ f₂ : α → β} {a₁ a₂ : α} (h₁ : Eq f₁ f₂) (h₂ : Eq a₁ a₂) : Eq (f₁ a₁) (f₂ a₂) :=
  @Eq.trans _ _ _ _
    (@congrArg (α → β) β f₁ f₂ (fun f => f a₁) h₁)
    (@congrArg α β a₁ a₂ f₂ h₂)

-- ── the logic tier: the Prop toolkit the emitter names ──────────────
-- Lean connective SHAPES that statements need; the event-level connectives a
-- corpus defines (`¬`, `∧`, `∨`, `⟹`, `⟺` as cited definitions) are the
-- corpus's own business, not this file's.
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
