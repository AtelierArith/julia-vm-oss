# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/array_axes_zero_dim_wrapper_6650.jl =====
module Agg_array_axes_zero_dim_wrapper_6650
using Test

@testset "Array wrapper axes and zero-dimensional indexing (#6650)" begin
    scalar = Array{Int64,0}(undef, ())
    setindex!(scalar, 42)

    @test axes(scalar) == ()
    @test axes(scalar, 1) == Base.OneTo(1)
    @test getindex(scalar) == 42
    @test setindex!(scalar, 7) === scalar
    @test getindex(scalar) == 7

    vector = [1, 2, 3]
    vector_axes = axes(vector)
    # Compatibility note (#6685): compare tuple axes component-wise until tuple
    # equality over OneTo struct elements matches upstream Julia.
    @test length(vector_axes) == 1
    @test first(vector_axes[1]) == 1
    @test last(vector_axes[1]) == 3
    @test length(vector_axes[1]) == 3
    @test axes(vector, 1) == Base.OneTo(3)
    @test axes(vector, 2) == Base.OneTo(1)

    matrix = [1 2; 3 4]
    matrix_axes = axes(matrix)
    @test length(matrix_axes) == 2
    @test first(matrix_axes[1]) == 1
    @test last(matrix_axes[1]) == 2
    @test length(matrix_axes[1]) == 2
    @test first(matrix_axes[2]) == 1
    @test last(matrix_axes[2]) == 2
    @test length(matrix_axes[2]) == 2
    @test axes(matrix, 1) == Base.OneTo(2)
    @test axes(matrix, 2) == Base.OneTo(2)
    @test axes(matrix, 3) == Base.OneTo(1)
end
end # module Agg_array_axes_zero_dim_wrapper_6650

# ===== source: array/array_collect_wrapper_routing_6649.jl =====
module Agg_array_collect_wrapper_routing_6649
using Test

@testset "collect routes public materialization to Array wrapper (#6649)" begin
    range_values = collect(1:3)
    @test typeof(range_values) === Vector{Int64}
    @test typeof(range_values.ref) == MemoryRef{Int64}
    @test range_values.size == (3,)
    @test range_values == [1, 2, 3]

    stepped = collect(1:2:7)
    @test typeof(stepped) === Vector{Int64}
    @test typeof(stepped.ref) == MemoryRef{Int64}
    @test stepped == [1, 3, 5, 7]

    floats = collect(1.0:0.5:2.0)
    @test typeof(floats) === Vector{Float64}
    @test typeof(floats.ref) == MemoryRef{Float64}
    @test floats == [1.0, 1.5, 2.0]

    tuple_values = collect((1, 2.5))
    @test typeof(tuple_values) === Vector{Real}
    @test typeof(tuple_values.ref) == MemoryRef{Real}
    @test eltype(tuple_values) == Real
    @test tuple_values[1] == 1
    @test tuple_values[2] == 2.5

    source = Int16[1, 2]
    copied = collect(source)
    @test typeof(copied) === Vector{Int16}
    @test typeof(copied.ref) == MemoryRef{Int16}
    copied[1] = Int16(9)
    @test source[1] == Int16(1)
    @test copied == Int16[9, 2]
end
end # module Agg_array_collect_wrapper_routing_6649

# ===== source: array/array_construction_remaining_routing_6649.jl =====
module Agg_array_construction_remaining_routing_6649
using Test

function array_dyn_6649(x)
    if x == 1
        return 1
    end
    return 2.5
end

@testset "remaining array construction routes to Array wrapper (#6649)" begin
    typed = Int8[1, 2, 3]
    @test typeof(typed.ref) == MemoryRef{Int8}
    @test typed.size == (3,)
    @test eltype(typed) == Int8
    @test typed[2] == Int8(2)

    empty_ctor = Vector{Int64}()
    @test typeof(empty_ctor.ref) == MemoryRef{Int64}
    @test empty_ctor.size == (0,)
    @test length(empty_ctor) == 0

    comp = [i * i for i in 1:4]
    @test typeof(comp.ref) == MemoryRef{Int64}
    @test comp.size == (4,)
    @test comp[4] == 16

    typed_comp = Float32[i for i in 1:3]
    @test typeof(typed_comp.ref) == MemoryRef{Float32}
    @test typed_comp.size == (3,)
    @test typed_comp[2] == Float32(2)

    typejoined = [array_dyn_6649(i) for i in 1:2 if true]
    @test typeof(typejoined.ref) == MemoryRef{Real}
    @test eltype(typejoined) == Real
    @test typejoined[1] == 1
    @test typejoined[2] == 2.5

    pairs = [(1, 10), (2, 20)]
    destructured = [a + b for (a, b) in pairs]
    @test typeof(destructured.ref) == MemoryRef{Int64}
    @test destructured[1] == 11
    @test destructured[2] == 22

    mixed_pairs = [(1, 10), (2, 20.5)]
    mixed_destructured = [a + b for (a, b) in mixed_pairs]
    @test typeof(mixed_destructured.ref) == MemoryRef{Real}
    @test eltype(mixed_destructured) == Real
    @test mixed_destructured[1] == 11
    @test mixed_destructured[2] == 22.5

    undefed = Array{Int16}(undef, 2)
    @test typeof(undefed.ref) == MemoryRef{Int16}
    @test undefed.size == (2,)

    z = zeros(Int32, 2)
    @test typeof(z.ref) == MemoryRef{Int32}
    @test z == Int32[0, 0]

    f = fill(Int16(7), 2)
    @test typeof(f.ref) == MemoryRef{Int16}
    @test f == Int16[7, 7]

    s = similar(typed, 2)
    @test typeof(s.ref) == MemoryRef{Int8}
    @test s.size == (2,)

    t = trues(2)
    @test length(t) == 2
    @test t[1] == true

    ff = falses(2)
    @test length(ff) == 2
    @test ff[1] == false
end
end # module Agg_array_construction_remaining_routing_6649

# ===== source: array/array_gen_dispatch_first_6744.jl =====
module Agg_array_gen_dispatch_first_6744
# Issue #6744 (#6729-2): the array generation functions zeros / ones / similar /
# reshape dispatch-first to their pure-Julia methods (base/array.jl). zeros/ones
# became pure-Julia allocation dispatch in Issue #4036 (BuiltinOp::Zeros/Ones are
# "dead but kept"); similar/reshape are pure-Julia `where {T,N}` methods. The
# legacy Rust array-creation builtins (Zeros/ZerosF64/ZerosI64/Ones/OnesF64/
# OnesI64) only ever allocate Float64 or Int64, so producing arrays of *other*
# element types proves the generic pure-Julia `zeros(::Type{T}, ...)` path is what
# runs (not the Rust fallback). Verified vs julia 1.12.

using Test

@testset "zeros/ones element type beyond F64/I64 → pure-Julia dispatch (Issue #6744)" begin
    # The Rust builtins can only make Float64/Int64 arrays; these types prove the
    # generic pure-Julia method (via _array_undef_from_dims + fill!) is used.
    @test zeros(Float32, 3) == Float32[0, 0, 0]
    @test eltype(zeros(Float32, 3)) === Float32
    @test zeros(Int32, 2, 2) == Int32[0 0; 0 0]
    @test eltype(zeros(Int32, 2, 2)) === Int32
    # NB: compared against `[0.0+0.0im, 0.0+0.0im]`, not `ComplexF64[0, 0]` —
    # the latter literal mis-stores Int elements in sjulia (tracked by #6771);
    # the zeros() result itself is correct ComplexF64.
    @test zeros(Complex{Float64}, 2) == [0.0 + 0.0im, 0.0 + 0.0im]
    @test eltype(zeros(Complex{Float64}, 2)) === ComplexF64
    @test ones(Float32, 3) == Float32[1, 1, 1]
    @test eltype(ones(Float32, 3)) === Float32
    @test ones(Int32, 2) == Int32[1, 1]
    @test eltype(ones(Int32, 2)) === Int32
end

@testset "zeros/ones defaults and basic forms (Issue #6744)" begin
    @test zeros(3) == [0.0, 0.0, 0.0]
    @test eltype(zeros(3)) === Float64        # default element type
    @test zeros(2, 3) == [0.0 0.0 0.0; 0.0 0.0 0.0]
    @test zeros(Int64, 2) == [0, 0]
    @test eltype(zeros(Int64, 2)) === Int64
    @test ones(2, 2) == [1.0 1.0; 1.0 1.0]
    @test ones(Int64, 3) == [1, 1, 1]
    @test zeros((2, 2)) == [0.0 0.0; 0.0 0.0]  # tuple-dims form
end

@testset "similar dispatch-first (Issue #6744)" begin
    a = [1 2 3; 4 5 6]
    @test size(similar(a)) == (2, 3)
    @test eltype(similar(a)) === Int64
    @test eltype(similar(a, Float64)) === Float64
    @test size(similar(a, (3, 2))) == (3, 2)
    @test size(similar(a, Float32, 4)) == (4,)
    @test eltype(similar(a, Float32, 4)) === Float32
end

@testset "reshape dispatch-first (Issue #6744)" begin
    a = [1 2 3; 4 5 6]
    @test size(reshape(a, 3, 2)) == (3, 2)
    @test reshape(a, 3, 2) == [1 5; 4 3; 2 6]   # column-major
    @test size(reshape(a, (6,))) == (6,)
    @test collect(reshape(1:6, 2, 3)) == [1 3 5; 2 4 6]
end
end # module Agg_array_gen_dispatch_first_6744

# ===== source: array/array_generator_collect_wrapper_routing_6649.jl =====
module Agg_array_generator_collect_wrapper_routing_6649
using Test

@testset "generator collect routes public materialization to Array wrapper (#6649)" begin
    eager = collect(x + 1 for x in 1:3)
    @test typeof(eager) === Vector{Int64}
    @test typeof(eager.ref) == MemoryRef{Int64}
    @test eager == [2, 3, 4]

    runtime_callable = collect(Base.Generator(x -> x + 1, 1:3))
    @test typeof(runtime_callable) === Vector{Int64}
    @test typeof(runtime_callable.ref) == MemoryRef{Int64}
    @test runtime_callable == [2, 3, 4]

    f(x) = x + 1
    function_callable = collect(Base.Generator(f, 1:3))
    @test typeof(function_callable) === Vector{Int64}
    @test typeof(function_callable.ref) == MemoryRef{Int64}
    @test function_callable == [2, 3, 4]

    filtered = collect(x + 10 for x in 1:5 if isodd(x))
    @test typeof(filtered) === Vector{Int64}
    @test typeof(filtered.ref) == MemoryRef{Int64}
    @test filtered == [11, 13, 15]

    tuple_splat = collect(x + y for (x, y) in zip(1:3, 4:6))
    @test typeof(tuple_splat) === Vector{Int64}
    @test typeof(tuple_splat.ref) == MemoryRef{Int64}
    @test tuple_splat == [5, 7, 9]
end
end # module Agg_array_generator_collect_wrapper_routing_6649

# ===== source: array/array_hof_broadcast_wrapper_6652.jl =====
module Agg_array_hof_broadcast_wrapper_6652
using Test

function make_memory_vector_6652(::Type{T}, values) where T
    mem = Memory{T}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, mem, length(values))
end

function make_offset_vector_6652(::Type{T}, values, start, len) where T
    mem = Memory{T}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, memoryref(mem, start), len)
end

function make_memory_matrix_6652(::Type{T}, values, dims) where T
    mem = Memory{T}(undef, length(values))
    for i in 1:length(values)
        mem[i] = values[i]
    end
    return Base.wrap(Array, mem, dims)
end

function is_memoryref_array_6652(a, ::Type{T}) where T
    return isa(a, Array) && typeof(a.ref) == MemoryRef{T}
end

@testset "Array wrapper HOF and broadcast over MemoryRef (#6652)" begin
    a = make_memory_vector_6652(Int64, [1, 2, 3, 4, 5])
    offset = make_offset_vector_6652(Int64, [10, 20, 30, 40, 50, 60], 3, 3)

    copied = collect(a)
    @test copied == [1, 2, 3, 4, 5]
    @test is_memoryref_array_6652(copied, Int64)

    offset_copy = collect(offset)
    @test offset_copy == [30, 40, 50]
    @test is_memoryref_array_6652(offset_copy, Int64)

    mapped = map(x -> x + 1, a)
    @test mapped == [2, 3, 4, 5, 6]
    @test is_memoryref_array_6652(mapped, Int64)

    mapped_binary = map(+, offset, make_memory_vector_6652(Int64, [1, 2, 3]))
    @test mapped_binary == [31, 42, 53]
    @test is_memoryref_array_6652(mapped_binary, Int64)

    map_dest = similar(a)
    @test map!(x -> x * 2, map_dest, a) === map_dest
    @test map_dest == [2, 4, 6, 8, 10]
    @test is_memoryref_array_6652(map_dest, Int64)

    map_binary_dest = similar(offset)
    @test map!(+, map_binary_dest, offset, make_memory_vector_6652(Int64, [1, 1, 1])) === map_binary_dest
    @test map_binary_dest == [31, 41, 51]
    @test is_memoryref_array_6652(map_binary_dest, Int64)

    filtered = filter(isodd, a)
    @test filtered == [1, 3, 5]
    @test is_memoryref_array_6652(filtered, Int64)

    filter_dest = collect(a)
    @test filter!(x -> x > 2, filter_dest) === filter_dest
    @test filter_dest == [3, 4, 5]
    @test is_memoryref_array_6652(filter_dest, Int64)

    @test reduce(+, a) == 15
    @test reduce(*, make_memory_vector_6652(Int64, [2, 3, 4])) == 24
    @test mapreduce(x -> x * 2, +, a) == 30

    generator_collect = collect(x * 2 for x in a)
    @test generator_collect == [2, 4, 6, 8, 10]
    @test is_memoryref_array_6652(generator_collect, Int64)

    comp = [x * 3 for x in a]
    @test comp == [3, 6, 9, 12, 15]
    @test is_memoryref_array_6652(comp, Int64)

    filtered_comp = [x * 3 for x in a if isodd(x)]
    @test filtered_comp == [3, 9, 15]
    @test is_memoryref_array_6652(filtered_comp, Int64)

    sorted = sort(make_memory_vector_6652(Int64, [4, 2, 5, 1, 3]))
    @test sorted == [1, 2, 3, 4, 5]
    @test is_memoryref_array_6652(sorted, Int64)

    sorted_rev = sort(make_memory_vector_6652(Int64, [4, 2, 5, 1, 3]); rev=true)
    @test sorted_rev == [5, 4, 3, 2, 1]
    @test is_memoryref_array_6652(sorted_rev, Int64)

    broadcasted = broadcast(x -> x + 10, a)
    @test broadcasted == [11, 12, 13, 14, 15]
    @test is_memoryref_array_6652(broadcasted, Int64)

    dotted = a .+ 2
    @test dotted == [3, 4, 5, 6, 7]
    @test is_memoryref_array_6652(dotted, Int64)

    broadcast_binary = broadcast(+, a, a)
    @test broadcast_binary == [2, 4, 6, 8, 10]
    @test is_memoryref_array_6652(broadcast_binary, Int64)

    broadcast_dest = similar(a)
    @test broadcast!(x -> x + 3, broadcast_dest, a) === broadcast_dest
    @test broadcast_dest == [4, 5, 6, 7, 8]
    @test is_memoryref_array_6652(broadcast_dest, Int64)

    mat = make_memory_matrix_6652(Int64, [1, 2, 3, 4, 5, 6], (2, 3))
    mat_inc = broadcast(x -> x + 1, mat)
    @test size(mat_inc) == (2, 3)
    @test mat_inc == [2 4 6; 3 5 7]
    @test is_memoryref_array_6652(mat_inc, Int64)

    mat_scalar = broadcast(+, mat, 10)
    @test mat_scalar == [11 13 15; 12 14 16]
    @test is_memoryref_array_6652(mat_scalar, Int64)
end
end # module Agg_array_hof_broadcast_wrapper_6652

# ===== source: array/array_index_store_dispatch_helper_3908.jl =====
module Agg_array_index_store_dispatch_helper_3908
using Test

@testset "IndexStore routes Tuple/Array/boxed targets through helper (Issue #3908)" begin
    # Tuple-value IndexStore branch: store a Tuple into a Vector{Tuple{Int,Int}}.
    tuple_storage = Vector{Tuple{Int,Int}}(undef, 2)
    tuple_storage[1] = (1, 2)
    tuple_storage[2] = (3, 4)

    @test tuple_storage[1] == (1, 2)
    @test tuple_storage[2] == (3, 4)
    @test length(tuple_storage) == 2

    # Array-element IndexStore branch (Issue #3648): store a Vector{Int} into a
    # heterogeneous Vector{Any}.
    nested = Vector{Any}(undef, 2)
    nested[1] = [10, 20]
    nested[2] = [30, 40, 50]

    @test nested[1] == [10, 20]
    @test nested[2] == [30, 40, 50]
    @test length(nested) == 2
    @test length(nested[2]) == 3

    # Boxed-scalar IndexStore branch (String/Char/Symbol path).
    str_storage = Vector{String}(undef, 2)
    str_storage[1] = "alpha"
    str_storage[2] = "beta"

    @test str_storage[1] == "alpha"
    @test str_storage[2] == "beta"

    char_storage = Vector{Char}(undef, 3)
    char_storage[1] = 'a'
    char_storage[2] = 'b'
    char_storage[3] = 'c'

    @test char_storage[1] == 'a'
    @test char_storage[2] == 'b'
    @test char_storage[3] == 'c'

    sym_storage = Vector{Symbol}(undef, 2)
    sym_storage[1] = :first
    sym_storage[2] = :second

    @test sym_storage[1] === :first
    @test sym_storage[2] === :second
end
end # module Agg_array_index_store_dispatch_helper_3908

# ===== source: array/array_literal_struct_routing_6649.jl =====
module Agg_array_literal_struct_routing_6649
using Test

@testset "array literal construction routes to Array wrapper (#6649)" begin
    a = [1, 2, 3]
    @test typeof(a.ref) == MemoryRef{Int64}
    @test a.size == (3,)
    @test a[2] == 2
    a[3] = 30
    @test a[3] == 30

    empty = Int64[]
    @test typeof(empty.ref) == MemoryRef{Int64}
    @test empty.size == (0,)
    @test length(empty) == 0

    m = [1 2; 3 4]
    @test typeof(m.ref) == MemoryRef{Int64}
    @test m.size == (2, 2)
    @test m[2, 1] == 3
    @test m[1, 2] == 2
end
end # module Agg_array_literal_struct_routing_6649

# ===== source: array/array_mutate_push_pop_helpers_3908.jl =====
module Agg_array_mutate_push_pop_helpers_3908
# Regression test for the array mutation re-push boundary in
# subset_julia_vm_vm/src/vm/exec/array_mutate.rs (Issue #3908). The Zero,
# ArrayPush, ArrayPop, ArrayPushFirst, ArrayPopFirst, ArrayInsert and
# ArrayDeleteAt handlers now route their `Value::Array(...)` construction
# through shared `push_array_ref` / `push_array_value` helpers. The behavior
# observed from Julia (return values, element types, lengths, shapes) must
# remain identical to native Julia for both Int64 and Float64 carriers.

using Test

@testset "Array mutate helpers (Issue #3908)" begin
    @testset "zero(::Array) preserves shape and element type" begin
        ints = [1, 2, 3]
        z_ints = zero(ints)
        @test z_ints == [0, 0, 0]
        @test length(z_ints) == 3
        @test eltype(z_ints) === Int64

        floats = [1.5, 2.5, 3.5]
        z_floats = zero(floats)
        @test z_floats == [0.0, 0.0, 0.0]
        @test length(z_floats) == 3
        @test eltype(z_floats) === Float64
    end

    @testset "push!/pop! round trip" begin
        xs = [1, 2, 3]
        push!(xs, 4)
        push!(xs, 5)
        @test xs == [1, 2, 3, 4, 5]
        @test length(xs) == 5

        last = pop!(xs)
        @test last == 5
        @test xs == [1, 2, 3, 4]
        @test length(xs) == 4
    end

    @testset "pushfirst!/popfirst! round trip" begin
        ys = [10, 20, 30]
        pushfirst!(ys, 5)
        pushfirst!(ys, 0)
        @test ys == [0, 5, 10, 20, 30]
        @test length(ys) == 5

        first_val = popfirst!(ys)
        @test first_val == 0
        @test ys == [5, 10, 20, 30]
        @test length(ys) == 4
    end

    @testset "insert!/deleteat! preserve order" begin
        zs = [1, 2, 4, 5]
        insert!(zs, 3, 3)
        @test zs == [1, 2, 3, 4, 5]
        @test length(zs) == 5

        deleteat!(zs, 1)
        @test zs == [2, 3, 4, 5]
        @test length(zs) == 4

        deleteat!(zs, length(zs))
        @test zs == [2, 3, 4]
        @test length(zs) == 3
    end

    @testset "mutations chain across helpers" begin
        ws = Float64[1.0, 2.0, 3.0]
        push!(ws, 4.0)
        pushfirst!(ws, 0.0)
        insert!(ws, 3, 1.5)
        @test ws == [0.0, 1.0, 1.5, 2.0, 3.0, 4.0]
        @test eltype(ws) === Float64

        deleteat!(ws, 3)
        @test ws == [0.0, 1.0, 2.0, 3.0, 4.0]
        last_val = pop!(ws)
        first_val = popfirst!(ws)
        @test last_val == 4.0
        @test first_val == 0.0
        @test ws == [1.0, 2.0, 3.0]
    end
end
end # module Agg_array_mutate_push_pop_helpers_3908

# ===== source: array/array_native_carrier_demoted_6653.jl =====
module Agg_array_native_carrier_demoted_6653
using Test

function is_memoryref_array_6653(a, ::Type{T}, dims) where T
    return isa(a, Array) && typeof(a.ref) == MemoryRef{T} && a.size == dims
end

function fill_linear_6653!(a)
    for i in 1:length(a)
        a[i] = i
    end
    return a
end

@testset "Public Array routes use MemoryRef-backed wrappers (#6653)" begin
    lit = [1, 2, 3]
    @test lit == [1, 2, 3]
    @test is_memoryref_array_6653(lit, Int64, (3,))

    typed_lit = Int64[4, 5, 6]
    @test typed_lit == [4, 5, 6]
    @test is_memoryref_array_6653(typed_lit, Int64, (3,))

    empty = Vector{Int64}()
    @test empty == Int64[]
    @test is_memoryref_array_6653(empty, Int64, (0,))

    undef_vec = fill_linear_6653!(Array{Int64}(undef, 3))
    @test undef_vec == [1, 2, 3]
    @test is_memoryref_array_6653(undef_vec, Int64, (3,))

    undef_mat = fill_linear_6653!(Array{Int64}(undef, (2, 2)))
    @test undef_mat == [1 3; 2 4]
    @test is_memoryref_array_6653(undef_mat, Int64, (2, 2))

    range_collect = collect(1:3)
    @test range_collect == [1, 2, 3]
    @test is_memoryref_array_6653(range_collect, Int64, (3,))

    tuple_collect = collect((1, 2, 3))
    @test tuple_collect == [1, 2, 3]
    @test is_memoryref_array_6653(tuple_collect, Int64, (3,))

    generator_collect = collect(x * 2 for x in lit)
    @test generator_collect == [2, 4, 6]
    @test is_memoryref_array_6653(generator_collect, Int64, (3,))

    comp = [x + 1 for x in lit]
    @test comp == [2, 3, 4]
    @test is_memoryref_array_6653(comp, Int64, (3,))

    mapped = map(x -> x + 1, lit)
    @test mapped == [2, 3, 4]
    @test is_memoryref_array_6653(mapped, Int64, (3,))

    filtered = filter(isodd, lit)
    @test filtered == [1, 3]
    @test is_memoryref_array_6653(filtered, Int64, (2,))

    sorted = sort([3, 1, 2])
    @test sorted == [1, 2, 3]
    @test is_memoryref_array_6653(sorted, Int64, (3,))

    broadcasted = broadcast(+, lit, lit)
    @test broadcasted == [2, 4, 6]
    @test is_memoryref_array_6653(broadcasted, Int64, (3,))

    similar_vec = similar(lit)
    @test is_memoryref_array_6653(similar_vec, Int64, (3,))

    zeros_vec = zeros(Int64, 3)
    @test zeros_vec == [0, 0, 0]
    @test is_memoryref_array_6653(zeros_vec, Int64, (3,))

    ones_vec = ones(Int64, 3)
    @test ones_vec == [1, 1, 1]
    @test is_memoryref_array_6653(ones_vec, Int64, (3,))

    reshaped = reshape([1, 2, 3, 4], (2, 2))
    @test reshaped == [1 3; 2 4]
    @test is_memoryref_array_6653(reshaped, Int64, (2, 2))
end
end # module Agg_array_native_carrier_demoted_6653

# ===== source: array/array_query_dispatch_first_6743.jl =====
module Agg_array_query_dispatch_first_6743
# Issue #6743 (#6729-1): the array query functions length / size / ndims /
# eltype dispatch-first to their pure-Julia methods (base/array.jl). The Rust
# builtins remain only as the no-method fallback for internal carriers, so a
# user-defined length/size/eltype method is NOT shadowed. Verified vs julia 1.12.

using Test

@testset "built-in length/size/ndims/eltype (Issue #6743)" begin
    a = [1 2 3; 4 5 6]
    @test length(a) == 6
    @test size(a) == (2, 3)
    @test size(a, 1) == 2
    @test ndims(a) == 2
    @test eltype(a) === Int64
    @test ndims([1, 2, 3]) == 1
    @test eltype([1.0, 2.0]) === Float64
    @test eltype(Float32[1, 2]) === Float32
    @test length("héllo") == 5     # character count
end

struct MyColl
    data::Vector{Int}
end
import Base: length, size, eltype, ndims
length(c::MyColl) = length(c.data)
size(c::MyColl) = (length(c.data),)
eltype(::Type{MyColl}) = Int
ndims(::MyColl) = 1

@testset "user-defined query methods are dispatch-first (Issue #6743)" begin
    c = MyColl([10, 20, 30, 40])
    @test length(c) == 4
    @test size(c) == (4,)
    @test eltype(MyColl) === Int
    @test ndims(c) == 1
    # works through a higher-order function too
    @test map(length, [MyColl([1]), MyColl([1, 2, 3])]) == [1, 3]
end
end # module Agg_array_query_dispatch_first_6743

# ===== source: array/build_buffer_devariant_6807.jl =====
module Agg_build_buffer_devariant_6807
# Issue #6807: the incremental array build buffer (NewArray*/PushElem*/Finalize*)
# is the last live VM producer of the legacy `Value::ExprArgs` carrier. It is
# emitted by the lazy specializer for typed array literals (`[1,2,3]`, etc.) and
# by the empty `Vector{String}` constants (ARGS/DEPOT_PATH/LOAD_PATH). This
# characterizes the value, element-type, ordering, mutation and special-layout
# (Complex / Tuple / struct) semantics of arrays produced through that build
# buffer so the de-variant onto the `Value::Memory` representation is provably
# behavior-preserving. Verified against upstream Julia 1.12.
using Test

struct Pt6807
    x::Int
    y::Int
end

# Force specialization by building the literals inside typed-arg functions.
make_int(a, b, c) = [a, b, c]
make_float(a, b) = [a, b]
make_bool(a, b) = [a, b]
make_str(a, b) = [a, b]
make_any() = [1, "two"]
make_complex(a, b) = [a, b]
make_tuple(a, b) = [a, b]
make_struct(a, b) = [a, b]

@testset "build buffer de-variant (Issue #6807)" begin
    # Int64 literal
    xi = make_int(10, 20, 30)
    @test xi == [10, 20, 30]
    @test eltype(xi) === Int64
    @test length(xi) == 3
    @test sum(xi) == 60
    push!(xi, 40)
    @test xi == [10, 20, 30, 40]

    # Float64 literal
    xf = make_float(1.5, 2.5)
    @test xf == [1.5, 2.5]
    @test eltype(xf) === Float64
    @test xf[2] === 2.5

    # Bool literal
    xb = make_bool(true, false)
    @test xb == [true, false]
    @test eltype(xb) === Bool
    @test count(xb) == 1

    # String literal
    xs = make_str("a", "b")
    @test xs == ["a", "b"]
    @test eltype(xs) === String
    @test xs[1] == "a"

    # Any (mixed) literal -> specializer's Any fallback
    xa = make_any()
    @test length(xa) == 2
    @test xa[1] === 1
    @test xa[2] == "two"
    @test eltype(xa) === Any

    # Complex literal -> boxed elements through the build buffer (value/ordering
    # parity; sjulia's specializer boxes complex literals as `Any` storage, so the
    # element *type* is intentionally not pinned here).
    xc = make_complex(1 + 2im, 3 + 4im)
    @test xc == [1 + 2im, 3 + 4im]
    @test real(xc[1]) == 1
    @test imag(xc[2]) == 4

    # Tuple-element literal -> AoS storage
    xt = make_tuple((1, 2), (3, 4))
    @test xt == [(1, 2), (3, 4)]
    @test xt[2] == (3, 4)
    @test length(xt) == 2

    # Struct-element literal -> heap struct refs
    xp = make_struct(Pt6807(1, 2), Pt6807(3, 4))
    @test length(xp) == 2
    @test xp[1].x == 1
    @test xp[2].y == 4

    # Empty typed array (the NewArrayTyped(_,0) + FinalizeArrayTyped path)
    empty_i = Int[]
    @test length(empty_i) == 0
    @test eltype(empty_i) === Int64
    push!(empty_i, 7)
    @test empty_i == [7]

    # ARGS is an empty Vector{String} built via the empty build-buffer path
    @test ARGS isa Vector{String}
    @test length(ARGS) == 0

    # 2-D literal goes through the build buffer with a rank-2 finalize shape
    m = [1 2; 3 4]
    @test size(m) == (2, 2)
    @test m[2, 1] == 3
    @test sum(m) == 10
end
end # module Agg_build_buffer_devariant_6807

# ===== source: array/builtins_arrays_query_helpers_3908.jl =====
module Agg_builtins_arrays_query_helpers_3908
# Regression test for the array query/construction boundary in
# subset_julia_vm_vm/src/vm/builtins_arrays.rs (Issue #3908). The Similar,
# Reshape, Size, Ndims, Keytype, and Valtype handlers now route their
# `Value::Array(...)` projection through a shared `value_as_array_ref`
# helper. The behavior observed from Julia (shape, ndims, element type, key
# type, value type, Complex-aware similar storage) must remain identical to
# native Julia across Int64, Float64, Bool, Complex{Float64}, and reshaped
# inputs.

using Test

@testset "Array query helpers (Issue #3908)" begin
    @testset "similar preserves element type and shape" begin
        ints = [1, 2, 3, 4]
        s_ints = similar(ints)
        @test eltype(s_ints) === Int64
        @test size(s_ints) == (4,)
        @test length(s_ints) == 4

        floats = [1.0 2.0; 3.0 4.0]
        s_floats = similar(floats)
        @test eltype(s_floats) === Float64
        @test size(s_floats) == (2, 2)
        @test ndims(s_floats) == 2

        bools = Bool[true, false, true]
        s_bools = similar(bools)
        @test eltype(s_bools) === Bool
        @test size(s_bools) == (3,)
    end

    @testset "similar(arr, T, dims) overrides element type and shape" begin
        ints = [1, 2, 3]
        s = similar(ints, Float64, 2, 2)
        @test eltype(s) === Float64
        @test size(s) == (2, 2)
        @test ndims(s) == 2
    end

    @testset "similar on Complex preserves Complex element type" begin
        cs = Array{ComplexF64}(undef, 2)
        s_cs = similar(cs)
        @test eltype(s_cs) === ComplexF64
        @test size(s_cs) == (2,)
        @test length(s_cs) == 2

        s_cs2 = similar(cs, 3, 2)
        @test eltype(s_cs2) === ComplexF64
        @test size(s_cs2) == (3, 2)
        @test ndims(s_cs2) == 2
    end

    @testset "reshape preserves element type and exposes new shape" begin
        xs = collect(1:6)
        m = reshape(xs, 2, 3)
        @test size(m) == (2, 3)
        @test ndims(m) == 2
        @test eltype(m) === Int64

        m2 = reshape(m, 3, 2)
        @test size(m2) == (3, 2)
        @test ndims(m2) == 2
        @test eltype(m2) === Int64
    end

    @testset "size/ndims report logical shape after reshape" begin
        xs = collect(1.0:8.0)
        m = reshape(xs, 2, 4)
        @test size(m) == (2, 4)
        @test size(m, 1) == 2
        @test size(m, 2) == 4
        @test ndims(m) == 2

        # size beyond ndims returns 1 (Julia convention)
        @test size(m, 3) == 1
    end

    @testset "keytype/valtype on arrays" begin
        ints = [10, 20, 30]
        @test keytype(ints) === Int64
        @test valtype(ints) === Int64

        floats = [1.5, 2.5]
        @test keytype(floats) === Int64
        @test valtype(floats) === Float64

        bools = Bool[true, false]
        @test keytype(bools) === Int64
        @test valtype(bools) === Bool

    end

    @testset "ndims of scalar/range/memory is unchanged" begin
        @test ndims(1) == 0
        @test ndims(1.5) == 0
        @test ndims(true) == 0
        @test ndims(1:5) == 1
    end
end
end # module Agg_builtins_arrays_query_helpers_3908

# ===== source: array/constructor_producers_wrapper_6807.jl =====
module Agg_constructor_producers_wrapper_6807
# Issue #6807: VM-level array producers — range materialization (`MakeRange` /
# `MakeRangeF64`), RNG arrays (`rand`/`randn`), and matrix ops — now emit the
# MemoryRef-backed `Array{T,N}` wrapper instead of the legacy `Value::ExprArgs`
# carrier. This characterizes value, element type, length, indexing and reductions
# on arrays produced by those instructions (the wrapper must behave exactly like
# any other array through `length`/`eltype`/indexing/`sum`). Verified against
# upstream Julia 1.12.
using Test

@testset "constructor producers as wrappers (Issue #6807)" begin
    # Integer range materialization
    xi = collect(1:5)
    @test xi == [1, 2, 3, 4, 5]
    @test eltype(xi) === Int64
    @test length(xi) == 5
    @test sum(xi) == 15
    @test xi[3] == 3

    # Stepped integer range
    xs = collect(2:2:10)
    @test xs == [2, 4, 6, 8, 10]
    @test length(xs) == 5

    # Float range materialization
    xf = collect(0.0:0.5:2.0)
    @test xf == [0.0, 0.5, 1.0, 1.5, 2.0]
    @test eltype(xf) === Float64
    @test length(xf) == 5

    # Matrix multiply result (value/length parity; the result *element type* of
    # an Int*Int matmul is a pre-existing sjulia gap — it widens to Float64 — so
    # it is intentionally not pinned here).
    m = [1 2; 3 4] * [1, 1]
    @test m == [3, 7]
    @test length(m) == 2

    # Matrix-matrix product
    mm = [1 2; 3 4] * [5 6; 7 8]
    @test mm == [19 22; 43 50]
    @test size(mm) == (2, 2)

    # RNG arrays: type/shape/range (values are non-deterministic across runtimes)
    r = rand(4)
    @test eltype(r) === Float64
    @test length(r) == 4
    @test all(x -> 0.0 <= x < 1.0, r)

    rn = randn(3)
    @test eltype(rn) === Float64
    @test length(rn) == 3

    # The produced arrays compose with higher-order array operations
    @test sum(map(x -> x * 2, collect(1:3))) == 12
    @test maximum(collect(1:10)) == 10
end
end # module Agg_constructor_producers_wrapper_6807

# ===== source: array/indexstore_logical_element_3908.jl =====
module Agg_indexstore_logical_element_3908
using Test

@testset "IndexStore uses logical array element type (Issue #3908)" begin
    complex_values = zeros(Complex{Float64}, 2)
    complex_values[1] = 2.5
    complex_values[2] = -3

    @test complex_values[1] == 2.5 + 0.0im
    @test complex_values[2] == -3.0 + 0.0im
    @test typeof(complex_values) == Vector{Complex{Float64}}
    @test typeof(complex_values[1]) == Complex{Float64}

    bool_values = Vector{Bool}(undef, 2)
    bool_values[1] = 1
    bool_values[2] = 0

    @test bool_values[1] == true
    @test bool_values[2] == false
    @test eltype(bool_values) == Bool
end
end # module Agg_indexstore_logical_element_3908

# ===== source: array/logical_index_load_helper_3908.jl =====
module Agg_logical_index_load_helper_3908
using Test

@testset "logical IndexLoad reads target through ArrayValue helper (Issue #3908)" begin
    # Logical (Bool) indexing on an Int64 vector exercises the
    # load_selected_array_elements path that now reads each selected
    # element via ArrayValue::get_linear instead of the multi-dim get,
    # so reshape-aware shared-backing semantics stay intact.
    data = [10, 20, 30, 40, 50]
    mask = [true, false, true, false, true]

    picked = data[mask]
    @test picked == [10, 30, 50]
    @test typeof(picked) == Vector{Int64}

    # Integer-index array selection through the same helper.
    integer_idx = [5, 1, 3]
    by_index = data[integer_idx]
    @test by_index == [50, 10, 30]
    @test typeof(by_index) == Vector{Int64}

    # Float64 selection confirms element type is preserved when the helper
    # reads logical f64 elements one at a time.
    float_data = [1.5, 2.5, 3.5, 4.5]
    float_mask = [false, true, false, true]
    float_picked = float_data[float_mask]
    @test float_picked == [2.5, 4.5]
    @test typeof(float_picked) == Vector{Float64}
end
end # module Agg_logical_index_load_helper_3908

# ===== source: array/memory_array_equality_boundary_3908.jl =====
module Agg_memory_array_equality_boundary_3908
# Regression test for the Memory<->Array equality boundary used by the
# binary_both dynamic dispatch fallback (Issue #3908). The fallback now
# reads memory cells through the public 1-indexed Memory boundary helper
# instead of touching MemoryValue::data directly, so the result must keep
# matching native Julia equality.

using Test

@testset "Memory and Array equality boundary (Issue #3908)" begin
    arr_int = [10, 20, 30]
    mem_int = Memory{Int64}(undef, 3)
    mem_int[1] = 10
    mem_int[2] = 20
    mem_int[3] = 30

    @test mem_int == arr_int
    @test arr_int == mem_int
    @test !(mem_int != arr_int)
    @test !(arr_int != mem_int)

    arr_float = [1.5, 2.5, 3.5]
    mem_float = Memory{Float64}(undef, 3)
    mem_float[1] = 1.5
    mem_float[2] = 2.5
    mem_float[3] = 3.5

    @test mem_float == arr_float
    @test arr_float == mem_float

    # Differ on the last cell only — must report inequality.
    mem_diff = Memory{Int64}(undef, 3)
    mem_diff[1] = 10
    mem_diff[2] = 20
    mem_diff[3] = 99
    @test mem_diff != arr_int
    @test arr_int != mem_diff
    @test !(mem_diff == arr_int)

    # Length mismatch — boundary must short-circuit to inequality without
    # reading past either side.
    mem_short = Memory{Int64}(undef, 2)
    mem_short[1] = 10
    mem_short[2] = 20
    @test mem_short != arr_int
    @test arr_int != mem_short
end
end # module Agg_memory_array_equality_boundary_3908

# ===== source: array/multidim_wrapper_index_untyped_param_6806.jl =====
module Agg_multidim_wrapper_index_untyped_param_6806
# Issue #6806 (PR B): indexing a MemoryRef-backed `Array{T,N}` wrapper through an
# untyped parameter (raw `IndexLoad`) reads the element directly from storage for
# both index modes `ArrayValue::linear_index` accepts — a single linear index of
# any rank, or one index per dimension (column-major) — instead of dispatching
# `getindex` per index. This extends the rank-1 fast path to multi-dimensional
# reads. Characterization of value and bounds semantics; verified against
# upstream Julia 1.12.
using Test

mid(m, i, j) = m[i, j]      # full N-index
lin(a, k) = a[k]            # single linear index (any rank)

@testset "multi-dim wrapper indexing via untyped param (Issue #6806)" begin
    m = [10i + j for i in 1:3, j in 1:4]   # 3x4 Matrix{Int64}, column-major

    # full per-dimension indexing
    @test mid(m, 1, 1) == 11
    @test mid(m, 3, 4) == 34
    @test mid(m, 2, 3) == 23

    # single linear index into a matrix (column-major order)
    @test lin(m, 1) == 11          # m[1,1]
    @test lin(m, 2) == 21          # m[2,1] (column-major)
    @test lin(m, 4) == 12          # m[1,2]
    @test lin(m, 12) == 34         # m[3,4]

    # 3-D array: full indexing and single linear index
    t = [100i + 10j + k for i in 1:2, j in 1:2, k in 1:2]
    @test t[1, 1, 1] == 111
    @test t[2, 2, 2] == 222
    @test lin(t, 1) == 111
    @test lin(t, 8) == 222

    # bounds errors preserved (type) for both modes
    @test_throws BoundsError mid(m, 4, 1)
    @test_throws BoundsError mid(m, 1, 5)
    @test_throws BoundsError lin(m, 13)
    @test_throws BoundsError lin(m, 0)

    # values stay typed
    mf = [Float64(i + j) for i in 1:2, j in 1:2]
    @test mid(mf, 2, 2) === 4.0
end
end # module Agg_multidim_wrapper_index_untyped_param_6806

# ===== source: array/rng_range_memory_first_3908.jl =====
module Agg_rng_range_memory_first_3908
using Test

@testset "RNG and range VM arrays stay Julia-visible (Issue #3908)" begin
    r = rand(2, 3)
    @test typeof(r) == Matrix{Float64}
    @test size(r) == (2, 3)
    @test length(r) == 6

    n = randn(4)
    @test typeof(n) == Vector{Float64}
    @test size(n) == (4,)
    @test length(n) == 4

    c = collect(1:3)
    @test typeof(c) == Vector{Int64}
    @test c == [1, 2, 3]
end
end # module Agg_rng_range_memory_first_3908

# ===== source: array/setindex_wrapper_untyped_param_6806.jl =====
module Agg_setindex_wrapper_untyped_param_6806
# Issue #6806 (PR B): writing into a numeric `Array{T}` wrapper through an untyped
# parameter (`a[i] = v`, compiled to a raw `IndexStore`) writes the element
# directly into the MemoryRef-backed storage instead of dispatching `setindex!`
# per write. Characterization of value, coercion, aliasing, and bounds semantics
# across that fast path; verified against upstream Julia 1.12.
using Test

setat!(a, i, v) = (a[i] = v; a)
getat(a, i) = a[i]

@testset "numeric wrapper setindex! via untyped param (Issue #6806)" begin
    # Int storage, Int value
    a = [10, 20, 30]
    setat!(a, 2, 99)
    @test a == [10, 99, 30]
    @test getat(a, 2) == 99

    # Float storage with Int value -> numeric convert to Float64 (matches setindex!)
    b = [1.0, 2.0, 3.0]
    setat!(b, 1, 7)
    @test b[1] === 7.0
    @test eltype(b) === Float64

    # Float value into Float storage
    setat!(b, 3, 3.5)
    @test b[3] === 3.5

    # aliasing: the wrapper is mutated in place, visible through another binding
    c = [0, 0, 0]
    d = c
    setat!(d, 2, 42)
    @test c[2] == 42

    # comprehension- and collect-built wrappers
    v = [i for i in 1:5]
    setat!(v, 5, 500)
    @test v[5] == 500
    w = collect(1:4)
    setat!(w, 1, -1)
    @test w[1] == -1

    # write to a linear position of a matrix wrapper (column-major)
    m = [10i + j for i in 1:2, j in 1:2]
    m[3] = 777          # linear index 3 == m[1,2]
    @test m[1, 2] == 777

    # bounds errors preserved (type)
    @test_throws BoundsError setat!([1, 2, 3], 5, 0)
    @test_throws BoundsError setat!([1, 2, 3], 0, 0)

    # the write returns the collection (IndexStore leaves it for StoreBack)
    r = setat!([1, 2, 3], 1, 9)
    @test r == [9, 2, 3]
end
end # module Agg_setindex_wrapper_untyped_param_6806

# ===== source: array/slice_index_logical_3908.jl =====
module Agg_slice_index_logical_3908
using Test

@testset "slice index arrays read logical reshaped elements (Issue #3908)" begin
    data = [10, 20, 30, 40]

    idx = [1, 4, 3, 4]
    reshaped_idx = reshape(idx, 4)
    selected = data[reshaped_idx]

    @test selected == [10, 40, 30, 40]
    @test typeof(selected) == Vector{Int64}

    mask = [true, false, true, false]
    reshaped_mask = reshape(mask, 4)
    mask[2] = true
    masked = data[reshaped_mask]

    @test masked == [10, 20, 30]
    @test typeof(masked) == Vector{Int64}
end
end # module Agg_slice_index_logical_3908

# ===== source: array/type_name_memoization_6846.jl =====
module Agg_type_name_memoization_6846
# Regression guard for Issue #6846 (type-name parse memoization in
# `CoreType::from_julia_name` + cheap array-wrapper type derivation in
# `array_wrapper_julia_type`). Exercises typeof / isa / eltype / dispatch across
# many distinct element types and ndims in one program, so a memoization cache
# that cross-contaminated entries, or a cheap base-name check that mis-classified
# a wrapper, would surface here. The repeated dispatch loop hits the warm cache.
using Test
using LinearAlgebra

@testset "array wrapper type identity stable under memoized parsing (#6846)" begin
    vf = [1.0, 2.0, 3.0]
    vi = [1, 2, 3]
    vb = [true, false]
    vc = [Complex(1.0, 2.0), Complex(3.0, 4.0)]
    m = [1.0 2.0; 3.0 4.0]

    @test typeof(vf) == Vector{Float64}
    @test typeof(vi) == Vector{Int}
    @test typeof(vb) == Vector{Bool}
    @test typeof(vc) == Vector{Complex{Float64}}
    @test typeof(m) == Matrix{Float64}

    @test vf isa AbstractVector
    @test vf isa AbstractArray
    @test !(vf isa AbstractMatrix)
    @test m isa AbstractMatrix
    @test vi isa AbstractVector{Int}
    @test !(vf isa Vector{Int})

    # Repeated dynamic dispatch over the wrappers (warm cache path).
    s = 0.0
    for _ in 1:50
        s += norm(vf) + norm(vi)
    end
    @test s ≈ 50 * (norm(vf) + norm(vi))

    @test eltype(vf) == Float64
    @test eltype(vc) == Complex{Float64}
    @test ndims(m) == 2
    @test ndims(vf) == 1
end
end # module Agg_type_name_memoization_6846

# ===== source: array/wrapper_compact_typeinfo_prefix_5774.jl =====
module Agg_wrapper_compact_typeinfo_prefix_5774
using Test

# Issue #5774: arrays allocated through the pure-Julia `Array` wrapper path
# (zeros / fill / ones / similar) dropped the upstream `T[...]` typeinfo prefix
# for non-implicit eltypes — only `Bool` was prefixed. `print`/`string`/`println`
# now emit the prefix for non-implicit scalar eltypes (Int8/Float32/ComplexF64/…)
# while implicit eltypes (Int64/Float64/Char/String/Symbol) and composite
# (Tuple) eltypes stay bare, matching `show`.

@testset "wrapper-compact array typeinfo prefix (Issue #5774)" begin
    # Non-implicit scalar eltypes gain the prefix
    @test string(zeros(Int8, 3)) == "Int8[0, 0, 0]"
    @test string(fill(Int16(5), 3)) == "Int16[5, 5, 5]"
    @test string(zeros(Float32, 2)) == "Float32[0.0, 0.0]"
    @test string(fill(1.0f0, 2)) == "Float32[1.0, 1.0]"
    @test string(zeros(ComplexF64, 2)) == "ComplexF64[0.0 + 0.0im, 0.0 + 0.0im]"

    # Implicit eltypes stay bare
    @test string(zeros(Int64, 2)) == "[0, 0]"
    @test string(zeros(2)) == "[0.0, 0.0]"

    # Empty arrays keep their type prefix
    @test string(Int8[]) == "Int8[]"

    # 2D matrix prefix
    @test string(zeros(Int8, 2, 2)) == "Int8[0 0; 0 0]"

    # Bool keeps its 1/0 element rendering + prefix
    @test string(trues(3)) == "Bool[1, 1, 1]"

    # Composite (Tuple) eltype stays bare (homogeneous-implicit), no spurious prefix
    @test string(fill((1, 2), 2)) == "[(1, 2), (1, 2)]"
end
end # module Agg_wrapper_compact_typeinfo_prefix_5774

# ===== source: array/wrapper_push_dispatch_4018.jl =====
module Agg_wrapper_push_dispatch_4018
using Test

function wrapper_push_dispatch_4018(a)
    b = similar(a, 0)
    push!(b, one(eltype(a)))
    push!(b, one(eltype(a)) + one(eltype(a)))
    return typeof(b) === Vector{eltype(a)} && length(b) == 2 && b[1] == 1 && b[2] == 2
end

@test wrapper_push_dispatch_4018([1, 2, 3])
end # module Agg_wrapper_push_dispatch_4018

# ===== source: array/wrapper_typed_index_dispatch_4018.jl =====
module Agg_wrapper_typed_index_dispatch_4018
using Test

function wrapper_typed_read_4018(a::Array{Int64})
    a[2, 1] + a[1, 2]
end

function wrapper_typed_write_4018!(a::Array{Int64})
    a[2, 2] = 44
    a[2, 2]
end

@testset "Array wrapper typed indexing dispatch (Issue #4018)" begin
    mem = Memory{Int64}(undef, 4)
    for i in 1:4
        mem[i] = i
    end

    a = Base.wrap(Array, mem, (2, 2))
    @test wrapper_typed_read_4018(a) == 5
    @test wrapper_typed_write_4018!(a) == 44
    @test mem[4] == 44
end
end # module Agg_wrapper_typed_index_dispatch_4018

true
