-- ============================================================================
-- thorn/pnp/lean/faux_pnp.lean — DOIT ÉCHOUER (comparer avec
-- thorn/pnp/preuves/p_eg_np_sans_preuve.thn, refusé par le pilote Thorn).
-- ============================================================================
-- On affirme l'égalité de deux propositions distinctes et on tente `rfl`.
-- Lean doit refuser, exactement comme Thorn refuse la réflexivité.

example : (∀ n : Nat, 0 + n = n) = (∀ n : Nat, n + 0 = n) := by
  rfl
