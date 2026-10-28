// Fused map+reduce : s = sum_i a[i]*a[i]  (une seule passe memoire)
module {
  func.func @sumsquares(%a: memref<?xi64>, %n: index) -> i64 {
    %c0 = arith.constant 0 : index
    %c1 = arith.constant 1 : index
    %zero = arith.constant 0 : i64
    %s = scf.for %i = %c0 to %n step %c1 iter_args(%acc = %zero) -> (i64) {
      %v = memref.load %a[%i] : memref<?xi64>
      %sq = arith.muli %v, %v : i64
      %nacc = arith.addi %acc, %sq : i64
      scf.yield %nacc : i64
    }
    return %s : i64
  }
}
