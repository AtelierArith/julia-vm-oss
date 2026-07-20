# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: array/arraymath_matrix_add_sub_dispatch_7579.jl =====
module Agg_arraymath_matrix_add_sub_dispatch_7579
using Test

@testset "Matrix arraymath addition and subtraction (Issue #7579)" begin
    A = [1.0 0.0; 0.0 2.0]
    @test A + A == [2.0 0.0; 0.0 4.0]
    S = A + A
    @test size(S, 1) == 2
    @test size(S, 2) == 2

    B = [1 2; 3 4]
    C = [0.5 1.5; 2.5 3.5]
    @test B + C == [1.5 3.5; 5.5 7.5]
    @test B - B == [0 0; 0 0]

    threw = false
    try
        [1 2] + [1; 2]
    catch
        threw = true
    end
    @test threw
end
end # module Agg_arraymath_matrix_add_sub_dispatch_7579

# NOTE: array/arraymath_vector_add_sub_dispatch_4019.jl is intentionally NOT
# aggregated here — it adds a method to Base.:+ on a Base argument type
# (`+(::Vector{Int64}, ::Vector{Int64})`, i.e. piracy). Method-table extension
# is process-global (even inside a @testset), so folding it in would leak the
# pirated `+` to later members and other aggregates. It stays standalone
# (#5966 class; Issue #10238).

# ===== source: array/map_bang_five_sources_4019.jl =====
module Agg_map_bang_five_sources_4019
using Test

map_bang_add5_4019(a, b, c, d, e) = a + b + c + d + e

function map_bang_runtime_4019(dest::Any, a::Any, b::Any, c::Any, d::Any, e::Any)
    return map!(map_bang_add5_4019, dest, a, b, c, d, e)
end

@testset "map! five-source Array dispatch (Issue #4019)" begin
    dest = [0, 0]
    result = map!(
        map_bang_add5_4019,
        dest,
        [1, 2],
        [10, 20],
        [100, 200],
        [1000, 2000],
        [10000, 20000],
    )
    @test result === dest
    @test dest == [11111, 22222]
    @test typeof(dest) === Vector{Int64}

    short_dest = [0, 0, 0]
    runtime = map_bang_runtime_4019(
        short_dest,
        [1, 2, 3],
        [10, 20],
        [100, 200, 300],
        [1000, 2000, 3000],
        [10000, 20000, 30000],
    )
    @test runtime === short_dest
    @test short_dest == [11111, 22222, 0]
    @test typeof(short_dest) === Vector{Int64}
end
end # module Agg_map_bang_five_sources_4019

# ===== source: array/map_bang_vararg_sources_4019.jl =====
module Agg_map_bang_vararg_sources_4019
using Test

map_bang_add6_4019(a, b, c, d, e, f) = a + b + c + d + e + f
map_bang_add9_4019(a, b, c, d, e, f, g, h, i) = a + b + c + d + e + f + g + h + i

function map_bang_vararg_runtime_4019(dest::Any, sources::Any)
    return map!(map_bang_add6_4019, dest, sources...)
end

@testset "map! vararg Array sources (Issue #4019)" begin
    dest = [0, 0]
    result = map!(
        map_bang_add6_4019,
        dest,
        [1, 2],
        [10, 20],
        [100, 200],
        [1000, 2000],
        [10000, 20000],
        [100000, 200000],
    )
    @test result === dest
    @test dest == [111111, 222222]
    @test typeof(dest) === Vector{Int64}

    splat_dest = [0, 0, 0]
    sources = ([1, 2, 3], [10, 20], [100, 200, 300], [1000, 2000, 3000], [10000, 20000, 30000], [100000, 200000, 300000])
    splat_result = map_bang_vararg_runtime_4019(splat_dest, sources)
    @test splat_result === splat_dest
    @test splat_dest == [111111, 222222, 0]

    fallback_dest = [0, 0]
    map!(
        map_bang_add9_4019,
        fallback_dest,
        [1, 2],
        [10, 20],
        [100, 200],
        [1000, 2000],
        [10000, 20000],
        [100000, 200000],
        [1000000, 2000000],
        [10000000, 20000000],
        [100000000, 200000000],
    )
    @test fallback_dest == [111111111, 222222222]
end
end # module Agg_map_bang_vararg_sources_4019

# ===== source: array/map_inplace_nary_4019.jl =====
module Agg_map_inplace_nary_4019
using Test

map_inplace_sum3_4019(x, y, z) = x + y + z
map_inplace_sum4_4019(w, x, y, z) = w + x + y + z
map_inplace_weighted3_4019(x, y, z) = x + 10y + 100z
map_inplace_weighted4_4019(w, x, y, z) = w + 10x + 100y + 1000z
map_inplace_div3_4019(x, y, z) = (x + y) / z

@testset "n-ary map! for arrays (Issue #4019)" begin
    dest = zeros(Int64, 4)
    a = [1, 2, 3, 4]
    b = [10, 20, 30, 40]
    c = [100, 200, 300, 400]
    result = map!(map_inplace_sum3_4019, dest, a, b, c)
    @test result === dest
    @test dest == [111, 222, 333, 444]

    short_dest = zeros(Int64, 2)
    short_result = map!(map_inplace_weighted3_4019, short_dest, [1, 2, 3], [4, 5, 6], [7, 8, 9])
    @test short_result === short_dest
    @test short_dest == [741, 852]

    long_dest = [0, 0, 0, 99]
    map!(map_inplace_sum3_4019, long_dest, [1, 2], [10, 20, 30], [100, 200, 300])
    @test long_dest == [111, 222, 0, 99]

    matrix_dest = zeros(Int64, 2, 2)
    map!(map_inplace_sum3_4019, matrix_dest, [1 2; 3 4], [10 20; 30 40], [100 200; 300 400])
    @test matrix_dest == [111 222; 333 444]

    float_dest = zeros(Float64, 3)
    map!(map_inplace_div3_4019, float_dest, [2, 3, 4], [2, 3, 4], [2, 2, 2])
    @test typeof(float_dest) === Vector{Float64}
    @test float_dest == [2.0, 3.0, 4.0]

    four_dest = zeros(Int64, 3)
    four_result = map!(map_inplace_sum4_4019, four_dest, [1, 2, 3], [10, 20, 30], [100, 200, 300], [1000, 2000, 3000])
    @test four_result === four_dest
    @test four_dest == [1111, 2222, 3333]

    four_short_dest = [0, 0, 0, 99]
    map!(map_inplace_weighted4_4019, four_short_dest, [1, 2], [3, 4, 5], [6, 7, 8], [9, 10, 11])
    @test four_short_dest == [9631, 10742, 0, 99]
end
end # module Agg_map_inplace_nary_4019

# ===== source: array/range_adjoint_similar_allocation_4018.jl =====
module Agg_range_adjoint_similar_allocation_4018
using Test

@testset "range adjoint allocation follows Array helper dispatch (Issues #4018, #4572)" begin
    r = range(1.0, step=0.5, length=4)
    a = adjoint(r)

    @test size(a) == (1, 4)
    @test a[1, 1] == 1.0
    @test a[1, 2] == 1.5
    @test a[1, 4] == 2.5

    lin = LinRange(1.0, 3.0, 3)
    lin_row = adjoint(lin)

    @test size(lin_row) == (1, 3)
    @test lin_row[1, 1] == 1.0
    @test lin_row[1, 2] == 2.0
    @test lin_row[1, 3] == 3.0
end
end # module Agg_range_adjoint_similar_allocation_4018

# NOTE: array/similar_any_receiver_dispatch_4018.jl is intentionally NOT
# aggregated here — it extends Base.similar with a method on a Base argument
# type (`similar(::Vector{Int64}, ::Int64)`, i.e. piracy). Method-table
# extension is process-global, so folding it into this aggregate would leak the
# pirated method to the later `similar_basic` member. It stays standalone
# (#5966 class; Issue #10238).

# ===== source: array/similar_bare_array_dispatch_4018.jl =====
module Agg_similar_bare_array_dispatch_4018
using Test

function bare_array_similar_shape_4018(a)
    b = similar(a, 0)
    push!(b, one(eltype(a)))
    push!(b, one(eltype(a)) + one(eltype(a)))
    return typeof(b) === Vector{eltype(a)} && b[1] == one(eltype(a)) && b[2] == one(eltype(a)) + one(eltype(a))
end

function bare_array_similar_tuple_shape_4018(a)
    b = similar(a, (2,))
    b[1] = zero(eltype(a))
    b[2] = one(eltype(a))
    return typeof(b) === Vector{eltype(a)} && b[1] == zero(eltype(a)) && b[2] == one(eltype(a))
end

@test bare_array_similar_shape_4018([1, 2, 3])
@test bare_array_similar_tuple_shape_4018([1, 2, 3])
@test vcat([1, 2], [3, 4]) == [1, 2, 3, 4]
end # module Agg_similar_bare_array_dispatch_4018

# ===== source: array/similar_basic.jl =====
module Agg_similar_basic
# similar() function for arrays (Issue #2129)
# Creates an uninitialized array with the same element type and shape (or specified size).

using Test

@testset "similar(a) - same type and shape (Issue #2129)" begin
    a = [1, 2, 3]
    b = similar(a)
    @test typeof(b) == Vector{Int64}
    @test length(b) == 3

    c = [1.0, 2.0, 3.0, 4.0]
    d = similar(c)
    @test typeof(d) == Vector{Float64}
    @test length(d) == 4
end

@testset "similar(a, n) - same type, different size (Issue #2129)" begin
    a = [1, 2, 3]
    b = similar(a, 5)
    @test typeof(b) == Vector{Int64}
    @test length(b) == 5

    c = [1.0, 2.0]
    d = similar(c, 10)
    @test typeof(d) == Vector{Float64}
    @test length(d) == 10

    # Zero-length array
    e = similar(a, 0)
    @test typeof(e) == Vector{Int64}
    @test length(e) == 0
end
end # module Agg_similar_basic

# ===== source: array/similar_multidim.jl =====
module Agg_similar_multidim
# similar(arr, dims...) for 2+ dimensions (Issue #3751)
# `similar(arr, n, m, ...)` returns an uninitialized array of eltype(arr)
# with the given shape. `similar(arr, T, n, m, ...)` returns an uninitialized
# array with element type T and the given shape.
# PR #3746 fixed the 1D case (similar(arr) and similar(arr, n)); the multi-dim
# arity was deferred to this issue.

using Test

@testset "similar(mat, n, m) returns 2D matrix (Issue #3751)" begin
    a = [1 2; 3 4]
    b = similar(a, 2, 3)
    @test typeof(b) == Matrix{Int64}
    @test size(b) == (2, 3)

    c = [1.0 2.0; 3.0 4.0]
    d = similar(c, 4, 5)
    @test typeof(d) == Matrix{Float64}
    @test size(d) == (4, 5)
end

@testset "similar(vec, n, m) reshapes Vector to Matrix" begin
    a = [1, 2, 3]
    b = similar(a, 2, 3)
    @test typeof(b) == Matrix{Int64}
    @test size(b) == (2, 3)

    c = [1.0, 2.0]
    d = similar(c, 3, 4)
    @test typeof(d) == Matrix{Float64}
    @test size(d) == (3, 4)
end

@testset "similar with 3+ dimensions (Issue #3751)" begin
    a = [1, 2, 3]
    b = similar(a, 2, 3, 4)
    @test typeof(b) == Array{Int64, 3}
    @test size(b) == (2, 3, 4)

    c = [1.0, 2.0]
    d = similar(c, 2, 2, 2, 2)
    @test typeof(d) == Array{Float64, 4}
    @test size(d) == (2, 2, 2, 2)
end

@testset "similar(arr, T, dims...) — typed multi-dim form" begin
    a = [1 2; 3 4]
    b = similar(a, Int, 4, 5)
    @test typeof(b) == Matrix{Int64}
    @test size(b) == (4, 5)

    c = [1, 2, 3]
    d = similar(c, Float64, 2, 3)
    @test typeof(d) == Matrix{Float64}
    @test size(d) == (2, 3)

    e = similar(c, Bool, 2, 2, 2)
    @test typeof(e) == Array{Bool, 3}
    @test size(e) == (2, 2, 2)
end

@testset "similar(arr, T) — typed same-shape form" begin
    a = [1, 2, 3]
    b = similar(a, Float64)
    @test typeof(b) == Vector{Float64}
    @test length(b) == 3

    c = [1 2; 3 4]
    d = similar(c, Bool)
    @test typeof(d) == Matrix{Bool}
    @test size(d) == (2, 2)
end

@testset "similar(arr, T, n) — typed 1D form" begin
    a = [1, 2, 3]
    b = similar(a, Float64, 5)
    @test typeof(b) == Vector{Float64}
    @test length(b) == 5
end

@testset "similar(arr, dims...) inside a function (Any-typed param)" begin
    f(arr) = similar(arr, 2, 3)
    r1 = f([1, 2, 3])
    @test typeof(r1) == Matrix{Int64}
    @test size(r1) == (2, 3)
    r2 = f([1.0, 2.0])
    @test typeof(r2) == Matrix{Float64}
    @test size(r2) == (2, 3)
end

@testset "similar with assignment writes back correctly" begin
    a = [1, 2, 3]
    b = similar(a, 2, 3)
    b[1, 1] = 10
    b[2, 3] = 99
    @test b[1, 1] == 10
    @test b[2, 3] == 99
end
end # module Agg_similar_multidim

# ===== source: array/similar_receiver_typevar_binding_4018.jl =====
module Agg_similar_receiver_typevar_binding_4018
using Test

function array_receiver_similar_type_4018(a::Array{T}) where T
    b = similar(a, 2)
    b[1] = zero(T)
    b[2] = one(T)
    return eltype(b) === T && b[1] == zero(T) && b[2] == one(T)
end

function memory_wrapper_receiver_similar_type_4018()
    mem = Memory{Int64}(undef, 3)
    a = Base.wrap(Array, mem, (3,))
    return array_receiver_similar_type_4018(a)
end

@test array_receiver_similar_type_4018([1, 2, 3])
@test memory_wrapper_receiver_similar_type_4018()
end # module Agg_similar_receiver_typevar_binding_4018

# ===== source: array/zeros_ones_fill_tuple_dims_4018.jl =====
module Agg_zeros_ones_fill_tuple_dims_4018
using Test

@testset "typed tuple-dims allocation follows Base array dispatch (Issue #4018)" begin
    zi = zeros(Int64, (2, 3))
    @test typeof(zi) === Matrix{Int64}
    @test size(zi) == (2, 3)
    @test zi[1, 1] == 0
    @test zi[2, 3] == 0

    oi = ones(Int64, (2, 3))
    @test typeof(oi) === Matrix{Int64}
    @test size(oi) == (2, 3)
    @test oi[1, 1] == 1
    @test oi[2, 3] == 1

    zc = zeros(Complex{Float64}, (2, 2))
    @test typeof(zc) === Matrix{Complex{Float64}}
    @test size(zc) == (2, 2)
    @test zc[1, 1] == 0.0 + 0.0im

    oc = ones(Complex{Float64}, (2, 2))
    @test typeof(oc) === Matrix{Complex{Float64}}
    @test size(oc) == (2, 2)
    @test oc[2, 2] == 1.0 + 0.0im

    filled = fill(7, (2, 3))
    @test typeof(filled) === Matrix{Int64}
    @test size(filled) == (2, 3)
    @test filled[1, 2] == 7
    @test filled[2, 3] == 7

    cube = fill(3, (2, 2, 2))
    @test typeof(cube) === Array{Int64, 3}
    @test size(cube) == (2, 2, 2)
    @test cube[2, 2, 2] == 3

    hyper_fill = fill(1, (1, 1, 1, 1))
    @test typeof(hyper_fill) === Array{Int64, 4}
    @test size(hyper_fill) == (1, 1, 1, 1)
    @test hyper_fill[1, 1, 1, 1] == 1

    hyper_zero = zeros(Int64, (1, 1, 1, 1))
    @test typeof(hyper_zero) === Array{Int64, 4}
    @test size(hyper_zero) == (1, 1, 1, 1)
    @test hyper_zero[1, 1, 1, 1] == 0

    real_similar = similar(Array{Real}, (2, 2))
    @test typeof(real_similar) === Matrix{Real}
    @test eltype(real_similar) === Real
    @test size(real_similar) == (2, 2)

    undef_matrix = Array{Int64}(undef, 2, 3)
    @test typeof(undef_matrix) === Matrix{Int64}
    @test eltype(undef_matrix) === Int64
    @test size(undef_matrix) == (2, 3)
    @test length(undef_matrix) == 6

    undef_vector = Array{Float64}(undef, (2,))
    @test typeof(undef_vector) === Vector{Float64}
    @test eltype(undef_vector) === Float64
    @test size(undef_vector) == (2,)
    @test length(undef_vector) == 2

    function generic_array_undef(T)
        result = Array{T}(undef, (2,))
        return (eltype(result), length(result), size(result))
    end

    generic_undef = generic_array_undef(Float64)
    @test generic_undef[1] === Float64
    @test generic_undef[2] == 2
    @test generic_undef[3] == (2,)

    function generic_similar_tuple_dims_4569(::Type{T}, dims::Tuple, expected_type, expected_eltype, expected_len) where T
        result = similar(Array{T}, dims)
        runtime_rank_type = Array{T,length(dims)}
        return typeof(result) === expected_type &&
               typeof(result) === runtime_rank_type &&
               eltype(result) === expected_eltype &&
               size(result) == dims &&
               length(result) == expected_len
    end

    @test generic_similar_tuple_dims_4569(Int64, (2,), Vector{Int64}, Int64, 2)
    @test generic_similar_tuple_dims_4569(Float64, (2, 3), Matrix{Float64}, Float64, 6)
    @test generic_similar_tuple_dims_4569(Bool, (1, 2), Matrix{Bool}, Bool, 2)
    @test generic_similar_tuple_dims_4569(Complex{Float64}, (2, 2), Matrix{Complex{Float64}}, Complex{Float64}, 4)

    function generic_similar_untyped_dims_4643(::Type{T}, dims, expected_type, expected_eltype, expected_len) where T
        result = similar(Array{T}, dims)
        runtime_rank_type = Array{T,length(dims)}
        return typeof(result) === expected_type &&
               typeof(result) === runtime_rank_type &&
               eltype(result) === expected_eltype &&
               size(result) == dims &&
               length(result) == expected_len
    end

    @test generic_similar_untyped_dims_4643(Int64, (2,), Vector{Int64}, Int64, 2)
    @test generic_similar_untyped_dims_4643(Float64, (2, 3), Matrix{Float64}, Float64, 6)
    @test generic_similar_untyped_dims_4643(Bool, (1, 2), Matrix{Bool}, Bool, 2)
    @test generic_similar_untyped_dims_4643(Complex{Float64}, (2, 2), Matrix{Complex{Float64}}, Complex{Float64}, 4)

    symbol_fill = fill(:ok, (1, 2))
    @test typeof(symbol_fill) === Matrix{Symbol}
    @test size(symbol_fill) == (1, 2)
    @test symbol_fill[1, 2] === :ok

    generic_symbols = fill(:ok, (1, 1, 1, 1))
    @test typeof(generic_symbols) === Array{Symbol, 4}
    @test size(generic_symbols) == (1, 1, 1, 1)
    @test generic_symbols[1, 1, 1, 1] === :ok
end
end # module Agg_zeros_ones_fill_tuple_dims_4018

true
