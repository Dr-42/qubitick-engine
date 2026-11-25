prelude

-- ═══════════════════════════════════════════════════════════════════
-- Qubitick.Core — the hand-written bare-prelude ground (directive #22/#26),
-- REDUCED per directive #29 (author canon, Part0_Bit_era.md):
--
--   Core depends on AND (∩), XOR (Δ), the nullity equality (∅-anchored =)
--   and tick (τ) as the → mechanism. That is it. No And/Or/etc.
--
-- The four mechanisms themselves live in the corpus (A1 axiom rows + the
-- class formations — 𝔈 ∩ Δ ∅ 𝟙 τ 𝔏 are all DB-driven). The ONLY Lean-side
-- ground Core must hand-carry is equality itself: kernel-native, root-name-
-- wired, and the license of every def-cite (MACRO-OUT lowers cited rewrites
-- to Eq.refl). Canonically equality is nullity-anchored: x = y ⟺ x Δ y = ∅
-- (a Δ a = ∅, a Δ ∅ = a are A1 rows; the characterization theorem is a T
-- row). The Lean Prop toolkit (False/True/Not/And/Or/Exists/Iff) is NOT a
-- Core dependency — it is HL surface, hand-grounded in Qubitick.HL
-- ("Rest all is HL surface", dir#29; canon declares ¬ ∧ ⟹ ⟺ at A3, and
-- the event-level connectives are corpus ring defs D.BT.*).
--
-- ZERO safe axioms: no propext, no funext, no Classical.choice, no
-- Quot.sound. Only `unsafe axiom` codegen markers (lcErased/lcAny), exactly
-- as Init/Prelude.lean does, at ROOT (namespaced placement fails — probed
-- 2026-11-03); audit policy (directive #22) permits exactly this shape.
-- Eq/rfl live at ROOT: the kernel's rfl-recognition is name-wired to
-- root `Eq`/`Eq.refl`.
-- ═══════════════════════════════════════════════════════════════════

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
