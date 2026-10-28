// MLIR standard-dialect representation of a reduction loop:
//   s = 0; for i in 0..N: s += a[i]
// État accumulateur en SSA (iter_args) pour permettre vectorisation/interleaving.
module {
  func.func @reduce(%a: memref<?xi64>, %n: index) -> i64 {
    %c0 = arith.constant 0 : index
    %c1 = arith.constant 1 : index
    %zero = arith.constant 0 : i64
    %s = scf.for %i = %c0 to %n step %c1 iter_args(%acc = %zero) -> (i64) {
      %v = memref.load %a[%i] : memref<?xi64>
      %nacc = arith.addi %acc, %v : i64
      scf.yield %nacc : i64
    }
    return %s : i64
  }
}
