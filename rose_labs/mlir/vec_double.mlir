// rose_labs/mlir/vec_double.mlir
// Démontre l'auto-vectorisation MLIR (affine-super-vectorize) sur une boucle
// élémentaire sur memref, puis lowering vers LLVM. À comparer à la version
// scalaire (vec_double_scalar) pour mesurer le gain.
module {
  func.func @vec_double(%buf: memref<?xf64>) {
    %c0 = arith.constant 0 : index
    %n = memref.dim %buf, %c0 : memref<?xf64>
    affine.for %i = 0 to %n {
      %v = memref.load %buf[%i] : memref<?xf64>
      %d = arith.mulf %v, %v : f64
      memref.store %d, %buf[%i] : memref<?xf64>
    }
    return
  }
}