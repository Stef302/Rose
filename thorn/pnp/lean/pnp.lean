-- ============================================================================
-- thorn/pnp/lean/pnp.lean — les mêmes énoncés jouets que thorn/pnp/preuves/*.thn
-- ============================================================================
-- Lean 4.36 CORE uniquement (aucun Mathlib), pour comparer avec Thorn.
-- Vérifier : lean thorn/pnp/lean/pnp.lean
-- ATTENTION : comme le prototype Thorn, ceci ne démontre PAS le vrai P vs NP.
--
-- Modèle jouet : entrée Nat ; D : Nat → Nat est un décideur (0 = accepte) ;
-- C : Nat → Nat fournit un certificat ; V(w,c) = D w + c.
--   P(D)  = ∀ w, D w = 0
--   NP(V) = ∀ w, D w + C w = C w

-- 1. L'inclusion facile, acceptée par Thorn (p_sous_np_jouet.thn).
theorem p_sous_np_jouet :
    ∀ (D C : Nat → Nat), (∀ w, D w = 0) → ∀ w, D w + C w = C w := by
  intro D C hP w
  calc
    D w + C w = 0 + C w := by rw [hP w]
    _ = C w := Nat.zero_add (C w)

-- 2. Les certificats, acceptés par Thorn (certificat_jouet.thn).
--    Nat.zero_add est le pendant de plus_0_l côté Thorn.
theorem certificat_zero : 0 + 0 = 0 := by rfl

theorem certificat_general : ∀ n : Nat, 0 + n = n := by
  intro n
  exact Nat.zero_add n

-- 3. La direction DURE, refusée par les tactiques Thorn actuelles
--    (np_sous_p_sans_preuve.thn), mais démontrable en Lean core en 4 lignes
--    grâce à l'annulation à droite de l'addition.
theorem np_sous_p_jouet :
    ∀ (D C : Nat → Nat), (∀ w, D w + C w = C w) → ∀ w, D w = 0 := by
  intro D C hV w
  have h : D w + C w = 0 + C w := by
    rw [hV w, Nat.zero_add]
  exact Nat.add_right_cancel h

-- 4. Ce que les DEUX systèmes acceptent trop facilement : poser l'égalité
--    comme axiome. Aucun vérificateur ne juge la vérité d'une supposition.
axiom p_eg_np_axiome :
    (∀ n : Nat, 0 + n = n) = (∀ n : Nat, n + 0 = n)

example : (∀ n : Nat, 0 + n = n) = (∀ n : Nat, n + 0 = n) :=
  p_eg_np_axiome
