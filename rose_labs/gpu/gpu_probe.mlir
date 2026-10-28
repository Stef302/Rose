// rose_labs/gpu/gpu_probe.mlir
// Squelette MLIR d'un kernel GPU (dialecte gpu) : double chaque élément.
// Non exécutable ici (pas de CUDA/ROCm), mais validé par mlir-opt-18 --verify-each.
module attributes {gpu.container_module} {
  gpu.module @kernels {
    gpu.func @double_kernel(%buf: memref<?xf64>) kernel {
      %c0 = arith.constant 0 : index
      %bid = gpu.block_id x
      %bsz = gpu.block_dim x
      %tid = gpu.thread_id x
      %gid = arith.muli %bid, %bsz : index
      %idx = arith.addi %gid, %tid : index
      %n = memref.dim %buf, %c0 : memref<?xf64>
      %inb = arith.cmpi slt, %idx, %n : index
      scf.if %inb {
        %v = memref.load %buf[%idx] : memref<?xf64>
        %d = arith.mulf %v, %v : f64
        memref.store %d, %buf[%idx] : memref<?xf64>
      }
      gpu.return
    }
  }

  func.func @host(%buf: memref<?xf64>) {
    %c0 = arith.constant 0 : index
    %c1 = arith.constant 1 : index
    %n = memref.dim %buf, %c0 : memref<?xf64>
    gpu.launch_func @kernels::@double_kernel
      blocks in (%n, %c1, %c1) threads in (%c1, %c1, %c1)
      args(%buf : memref<?xf64>)
    return
  }
}
