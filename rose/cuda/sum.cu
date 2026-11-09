extern "C" __global__ void sum_i64(const long long* input, unsigned long long* output, int n) {
    extern __shared__ long long sdata[];
    int tid = threadIdx.x;
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    long long val = 0;
    if (i < n) val = input[i];
    sdata[tid] = val;
    __syncthreads();
    for (int s = blockDim.x / 2; s > 0; s >>= 1) {
        if (tid < s) sdata[tid] += sdata[tid + s];
        __syncthreads();
    }
    if (tid == 0) atomicAdd(output, (unsigned long long)sdata[0]);
}
