# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/array_any_push_no_coerce_5717.jl =====
module Agg_array_any_push_no_coerce_5717
using Test

# Issue #5717: push!(Any[], <int>) stored the integer as Float64 — the push!
# compile path coerced I64/I32/F32 to F64 for any array that was not a typed
# non-F64 array, wrongly grouping `Any` arrays with legacy/F64 storage. An `Any`
# array stores values verbatim, so integers must be preserved.

@testset "push! into Any[] preserves element types (Issue #5717)" begin
    v = Any[]
    push!(v, 10)
    @test typeof(v[1]) == Int64
    @test v[1] == 10
    push!(v, 2.5)
    @test typeof(v[2]) == Float64
    push!(v, "x")
    @test typeof(v[3]) == String
    push!(v, :sym)
    @test typeof(v[4]) == Symbol

    # Non-empty Any array, then push an integer.
    u = Any[1]
    push!(u, 20)
    @test typeof(u[2]) == Int64
    @test u == Any[1, 20]

    # An untyped `[]` is `Vector{Any}`, so it also preserves the pushed Int.
    untyped = []
    push!(untyped, 7)
    @test typeof(untyped[1]) == Int64

    # Regression: a concretely-typed Float64 array still widens integers to Float64.
    f = Float64[]
    push!(f, 3)
    @test typeof(f[1]) == Float64

    # Regression: concretely-typed integer arrays preserve their width.
    iv = Int[]
    push!(iv, 9)
    @test typeof(iv[1]) == Int64

    i32 = Int32[]
    push!(i32, Int32(5))
    @test typeof(i32[1]) == Int32
end
end # module Agg_array_any_push_no_coerce_5717

# ===== source: array/array_memory_mutation_iteration_6651.jl =====
module Agg_array_memory_mutation_iteration_6651
using Test

function make_memory_vector_6651(values)
    mem = Memory{Int64}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, mem, length(values)), mem
end

function make_offset_vector_6651()
    mem = Memory{Int64}(undef, 6)
    for i in 1:6
        mem[i] = 10 * i
    end
    return Base.wrap(Array, memoryref(mem, 3), 3), mem
end

@testset "Array wrapper mutation and iteration over MemoryRef (#6651)" begin
    a, _ = make_memory_vector_6651([1, 2, 3])
    first_next = iterate(a)
    @test first_next == (1, 2)
    second_next = iterate(a, first_next[2])
    @test second_next == (2, 3)
    third_next = iterate(a, second_next[2])
    @test third_next == (3, 4)
    @test iterate(a, third_next[2]) === nothing

    total = 0
    for x in a
        total += x
    end
    @test total == 6

    @test push!(a, 4) === a
    @test collect(a) == [1, 2, 3, 4]
    @test pop!(a) == 4
    @test collect(a) == [1, 2, 3]
    @test pushfirst!(a, 0) === a
    @test collect(a) == [0, 1, 2, 3]
    @test popfirst!(a) == 0
    @test collect(a) == [1, 2, 3]
    @test insert!(a, 2, 9) === a
    @test collect(a) == [1, 9, 2, 3]
    @test deleteat!(a, 2) === a
    @test collect(a) == [1, 2, 3]
    @test append!(a, (4, 5)) === a
    @test collect(a) == [1, 2, 3, 4, 5]
    @test resize!(a, 3) === a
    @test collect(a) == [1, 2, 3]
    @test empty!(a) === a
    @test length(a) == 0

    pushed, pushed_mem = make_offset_vector_6651()
    @test collect(pushed) == [30, 40, 50]
    @test push!(pushed, 99) === pushed
    @test collect(pushed) == [30, 40, 50, 99]
    pushed[1] = 77
    @test pushed_mem[3] == 77

    popped, popped_mem = make_offset_vector_6651()
    @test pop!(popped) == 50
    @test collect(popped) == [30, 40]
    popped[1] = 71
    @test popped_mem[3] == 71

    shifted_first, shifted_first_mem = make_offset_vector_6651()
    @test popfirst!(shifted_first) == 30
    @test collect(shifted_first) == [40, 50]
    shifted_first[1] = 72
    @test shifted_first_mem[4] == 72

    prepended, prepended_mem = make_offset_vector_6651()
    @test pushfirst!(prepended, 22) === prepended
    @test collect(prepended) == [22, 30, 40, 50]
    prepended[1] = 73
    @test prepended_mem[2] == 73
    @test prepended_mem[3] == 30

    inserted, inserted_mem = make_offset_vector_6651()
    @test insert!(inserted, 2, 88) === inserted
    @test collect(inserted) == [30, 88, 40, 50]
    @test inserted_mem[3] == 30
    @test inserted_mem[4] == 88
    @test inserted_mem[5] == 40
    @test inserted_mem[6] == 50

    deleted_middle, deleted_middle_mem = make_offset_vector_6651()
    @test deleteat!(deleted_middle, 2) === deleted_middle
    @test collect(deleted_middle) == [30, 50]
    deleted_middle[1] = 74
    @test deleted_middle_mem[3] == 74
    @test deleted_middle_mem[4] == 50

    deleted_first, deleted_first_mem = make_offset_vector_6651()
    @test deleteat!(deleted_first, 1) === deleted_first
    @test collect(deleted_first) == [40, 50]
    deleted_first[1] = 75
    @test deleted_first_mem[3] == 30
    @test deleted_first_mem[4] == 75

    resized, resized_mem = make_offset_vector_6651()
    @test resize!(resized, 4) === resized
    resized[4] = 76
    @test resized_mem[6] == 76
    @test resize!(resized, 2) === resized
    resized[1] = 77
    @test resized_mem[3] == 77
end
end # module Agg_array_memory_mutation_iteration_6651

# ===== source: array/copyto_overlap.jl =====
module Agg_copyto_overlap
using Test

# Regression test for Issue #3595:
# `copyto!(dest, dstart, src, sstart, n)` (and the related 2/3/4-arg variants)
# must handle overlapping source/destination ranges like memmove. Forward-only
# iteration corrupts data when dest === src && dstart > sstart.

@testset "copyto! overlap (#3595)" begin
    # MWE from Issue: forward-overlap on the same array
    a = [1, 2, 3, 4]
    copyto!(a, 2, a, 1, 3)
    @test a == [1, 1, 2, 3]

    # Reverse-overlap on the same array (forward iteration is correct here)
    b = [1, 2, 3, 4]
    copyto!(b, 1, b, 2, 3)
    @test b == [2, 3, 4, 4]

    # Full self-copy (dstart == sstart) — no-op
    c = [1, 2, 3, 4]
    copyto!(c, 1, c, 1, 4)
    @test c == [1, 2, 3, 4]

    # Non-overlapping copy between distinct arrays — unchanged behavior
    d1 = [1, 2, 3]
    d2 = [10, 20, 30]
    copyto!(d1, 1, d2, 1, 3)
    @test d1 == [10, 20, 30]

    # 3-arg form: copyto!(dest, dstart, src) with non-overlapping arrays
    f = [1, 2, 3, 4]
    copyto!(f, 2, [10, 20])
    @test f == [1, 10, 20, 4]

    # 2-arg form: copyto!(dest, src) with distinct arrays
    g = [0, 0, 0]
    copyto!(g, [10, 20, 30])
    @test g == [10, 20, 30]

    # 4-arg with reverse overlap on same array
    h = [10, 20, 30, 40, 50]
    copyto!(h, 1, h, 3)        # copy h[3:end] to h[1:end-2]
    @test h == [30, 40, 50, 40, 50]

    # 3-arg dstart=1, dest === src — no-op (length mismatch caveat: dest must fit src)
    k = [1, 2, 3]
    copyto!(k, 1, k)
    @test k == [1, 2, 3]
end
end # module Agg_copyto_overlap

# ===== source: array/global_array_mutations.jl =====
module Agg_global_array_mutations
using Test

# Global arrays mutated via push!/pop!/pushfirst!/popfirst!/insert!/deleteat! inside functions
# Issue #3121: StoreArray inside functions caused slotization to shadow global arrays

const ACCUM = Int64[]
const LOG3 = [10, 20, 30]
const FRONT = [1, 2, 3]

function accumulate_val(v)
    push!(ACCUM, v)
end

function pop_last()
    pop!(LOG3)
end

function push_to_front(v)
    pushfirst!(FRONT, v)
end

function pop_from_front()
    popfirst!(FRONT)
end

const INS_ARR = [1, 3, 4]

function insert_middle(v)
    insert!(INS_ARR, 2, v)
end

const DEL_ARR = [10, 99, 20]

function delete_second()
    deleteat!(DEL_ARR, 2)
end

@testset "Global array mutations via functions (Issue #3121)" begin
    @testset "push! on global array" begin
        accumulate_val(5)
        accumulate_val(10)
        accumulate_val(15)
        @test length(ACCUM) == 3
        @test ACCUM[1] == 5
        @test ACCUM[2] == 10
        @test ACCUM[3] == 15
    end

    @testset "pop! on global array" begin
        val = pop_last()
        @test val == 30.0
        @test length(LOG3) == 2
    end

    @testset "pushfirst! on global array" begin
        push_to_front(0)
        @test FRONT[1] == 0
        @test FRONT[2] == 1
        @test length(FRONT) == 4
    end

    @testset "popfirst! on global array" begin
        val = pop_from_front()
        @test val == 0.0
        @test FRONT[1] == 1
        @test length(FRONT) == 3
    end

    @testset "insert! on global array" begin
        insert_middle(2)
        @test INS_ARR[1] == 1
        @test INS_ARR[2] == 2
        @test INS_ARR[3] == 3
        @test length(INS_ARR) == 4
    end

    @testset "deleteat! on global array" begin
        delete_second()
        @test length(DEL_ARR) == 2
        @test DEL_ARR[1] == 10
        @test DEL_ARR[2] == 20
    end
end
end # module Agg_global_array_mutations

# ===== source: array/global_array_patterns.jl =====
module Agg_global_array_patterns
using Test

# Global const arrays must be at top-level scope.

const PRIMES = [2, 3, 5, 7, 11, 13]
const ACC = [0]
const LOG2 = Int64[]
const TEMPS = [36.5, 37.0, 36.8, 37.2]

function add_to_acc(x)
    ACC[1] += x
end

function log_value_idx(i, v)
    if i > length(LOG2)
        push!(LOG2, v)
    else
        LOG2[i] = v
    end
end

@testset "Global array patterns" begin
    @testset "read-only global const arrays" begin
        @test length(PRIMES) == 6
        @test PRIMES[1] == 2
        @test PRIMES[end] == 13
        @test sum(PRIMES) == 41
    end

    @testset "global const array mutation via functions" begin
        add_to_acc(10)
        add_to_acc(20)
        add_to_acc(5)
        @test ACC[1] == 35
    end

    @testset "global const array element mutation" begin
        const_arr = [10, 20, 30]
        const_arr[2] = 99
        @test const_arr[2] == 99
        @test const_arr[1] == 10
        @test const_arr[3] == 30
    end

    @testset "Float64 global array" begin
        @test minimum(TEMPS) == 36.5
        @test maximum(TEMPS) == 37.2
        @test length(TEMPS) == 4
    end

    @testset "push! via function on typed empty global array (Issue #3121)" begin
        log_value_idx(1, 100)
        log_value_idx(2, 200)
        @test length(LOG2) == 2
        @test LOG2[1] == 100
        @test LOG2[2] == 200
    end
end
end # module Agg_global_array_patterns

# ===== source: array/global_array_store_reload.jl =====
module Agg_global_array_store_reload
using Test

# Issue #3131: StoreArray-for-globals slotization hazard
# Tests that global arrays remain visible and modifiable after store/reload patterns

const STORE_A = [1, 2, 3]

function double_first_element()
    STORE_A[1] = STORE_A[1] * 2
end

function read_after_write(idx)
    STORE_A[idx] = STORE_A[idx] + 100
    return STORE_A[idx]
end

@testset "StoreArray global mutation (Issue #3131)" begin
    @testset "In-place element mutation" begin
        double_first_element()
        @test STORE_A[1] == 2
        double_first_element()
        @test STORE_A[1] == 4
    end

    @testset "Read-after-write in same function" begin
        result = read_after_write(2)
        @test result == 102
        @test STORE_A[2] == 102
    end
end

const MULTI_G = [10, 20, 30]

function swap_elements(i, j)
    tmp = MULTI_G[i]
    MULTI_G[i] = MULTI_G[j]
    MULTI_G[j] = tmp
end

@testset "Multi-step global array mutation" begin
    swap_elements(1, 3)
    @test MULTI_G[1] == 30
    @test MULTI_G[3] == 10
    @test MULTI_G[2] == 20
end

const ACC_INT = Int64[]

function accumulate_and_sum()
    push!(ACC_INT, 10)
    push!(ACC_INT, 20)
    push!(ACC_INT, 30)
    return ACC_INT[1] + ACC_INT[2] + ACC_INT[3]
end

@testset "Global array push and read in same function" begin
    s = accumulate_and_sum()
    @test s == 60
    @test length(ACC_INT) == 3
end
end # module Agg_global_array_store_reload

# ===== source: array/growth_amortized_6873.jl =====
module Agg_growth_amortized_6873
# Regression guard for Issue #6873 (found while profiling Issue #6846).
#
# Appending to an `Array{T}` wrapper now grows the backing `Memory` in place via
# the underlying Vec's amortized (geometric) growth, instead of reallocating an
# exact-size `Memory` and copying every prior element on each push — which made
# comprehensions and `push!` loops O(n^2). The in-place growth must preserve
# every element, its order, and its type across the (now internal) Vec
# reallocations that happen as the buffer doubles. This fixture stresses growth
# large enough to cross several reallocation boundaries and checks the result is
# byte-for-byte what upstream Julia 1.12 produces.

using Test

struct Pt
    x::Int
    y::Int
end

@testset "amortized array growth correctness (Issue #6873)" begin
    # --- large typed comprehension (Float64) ---
    n = 1000
    zf = Float64[Float64(i) for i in 1:n]
    @test length(zf) == n
    @test zf[1] == 1.0
    @test zf[500] == 500.0
    @test zf[n] == Float64(n)
    @test sum(zf) == n * (n + 1) / 2

    # --- large untyped comprehension (Int body) ---
    zi = [2 * i for i in 1:n]
    @test length(zi) == n
    @test zi[1] == 2
    @test zi[n] == 2 * n
    @test sum(zi) == n * (n + 1)

    # --- 2D comprehension (the surface-plot shape) ---
    m = 40
    z2 = Float64[Float64(i + 100 * j) for j in 1:m, i in 1:m]
    @test size(z2) == (m, m)
    @test z2[1, 1] == 101.0
    @test z2[m, m] == Float64(m + 100 * m)
    @test z2[3, 7] == Float64(7 + 100 * 3)

    # --- push! loop builds the same vector, in order ---
    a = Float64[]
    for i in 1:n
        push!(a, Float64(i * i))
    end
    @test length(a) == n
    @test a[1] == 1.0
    @test a[2] == 4.0
    @test a[n] == Float64(n * n)
    @test a[123] == Float64(123 * 123)

    # --- push! across element types preserves value + type ---
    ac = ComplexF64[]
    for i in 1:50
        push!(ac, Complex(Float64(i), Float64(-i)))
    end
    @test length(ac) == 50
    @test ac[1] == 1.0 - 1.0im
    @test ac[50] == 50.0 - 50.0im
    @test eltype(ac) == ComplexF64

    ap = Pt[]
    for i in 1:30
        push!(ap, Pt(i, 2 * i))
    end
    @test length(ap) == 30
    @test ap[1] == Pt(1, 2)
    @test ap[30] == Pt(30, 60)

    astrs = String[]
    for i in 1:20
        push!(astrs, string("v", i))
    end
    @test astrs[1] == "v1"
    @test astrs[20] == "v20"

    aany = []
    push!(aany, 1)
    push!(aany, "two")
    push!(aany, 3.0)
    push!(aany, Pt(4, 5))
    @test length(aany) == 4
    @test aany[2] == "two"
    @test aany[4] == Pt(4, 5)

    # --- mixed push!/index-mutate/push! must not corrupt across reallocation ---
    b = Int[]
    for i in 1:100
        push!(b, i)
    end
    b[1] = -1
    b[50] = -50
    for i in 101:200
        push!(b, i)
    end
    @test length(b) == 200
    @test b[1] == -1
    @test b[50] == -50
    @test b[100] == 100
    @test b[200] == 200

    # --- aliasing: `c = d` shares; push!(d, ...) is visible through c ---
    d = [10, 20, 30]
    c = d
    push!(d, 40)
    @test length(c) == 4
    @test c[4] == 40
end
end # module Agg_growth_amortized_6873

# ===== source: array/zeros_alloc_not_cse_7176.jl =====
module Agg_zeros_alloc_not_cse_7176
# Issue #7176: two textually-identical allocating calls (`zeros(n)`, `ones(n)`,
# `fill`, `similar`, `copy`, `collect`) must each return a fresh, independent
# array. They were classified as fully `:consistent`/pure, so CSE merged
# `a = zeros(n); b = zeros(n)` into a single shared allocation — mutating `a`
# also changed `b`. This produced a straight line instead of a Barnsley fern,
# because `xs = zeros(n); ys = zeros(n)` aliased the same buffer.
using Test

function two_zeros(n)
    a = zeros(n)
    b = zeros(n)
    a[1] = 5.0
    return (a[1], b[1])
end

function two_ones()
    a = ones(3)
    b = ones(3)
    a[1] = 7.0
    return b[1]
end

function two_fill()
    a = fill(2.0, 3)
    b = fill(2.0, 3)
    a[1] = 7.0
    return b[1]
end

function two_collect()
    a = collect(1:3)
    b = collect(1:3)
    a[1] = 99
    return b[1]
end

function two_copy()
    src = [1.0, 2.0, 3.0]
    a = copy(src)
    b = copy(src)
    a[1] = 7.0
    return (b[1], src[1])
end

@testset "Issue #7176: allocating calls are not CSE-merged" begin
    a1, b1 = two_zeros(3)
    @test a1 == 5.0
    @test b1 == 0.0            # b must stay untouched
    @test two_ones() == 1.0
    @test two_fill() == 2.0
    @test two_collect() == 1
    cb, cs = two_copy()
    @test cb == 1.0            # copy is independent of the mutated copy
    @test cs == 1.0            # and of the source
end
end # module Agg_zeros_alloc_not_cse_7176

true
