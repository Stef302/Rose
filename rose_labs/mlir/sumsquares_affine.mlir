// Affine dialect : somme des carrés avec vectorisation automatique.
#map = affine_map<(d0) -> (d0)>
module {
  func.func @sumsquares(%a: memref<?xi64>, %n: index) -> i64 {
    %c0 = arith.constant 0 : index
    %zero = arith.constant 0 : i64
    %s = affine.for %i = 0 to %n iter_args(%acc = %zero) -> (i64) {
      %v = affine.load %a[%i] : memref<?xi64>
      %sq = arith.muli %v, %v : i64
      %nacc = arith.addi %acc, %sq : i64
      affine.yield %nacc : i64
    }
    return %s : i64
  }
}
