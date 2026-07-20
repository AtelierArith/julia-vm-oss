# Aggregated concat-safe @testset fixtures (Issue #9671 Phase 3 expansion).
# Each block below is one former standalone fixture, verbatim except its
# `using Test` / trailing `true` were hoisted. @testset names (with their
# original Issue numbers) are preserved, and the #9360 @testset gate still
# detects any per-@testset failure. Source fixture in each banner.
using Test

# ===== source: type_inference/broadcast_inference.jl =====
# Test broadcast result type inference for common patterns (Issue #3464)


@testset "type_inference_broadcast_inference: broadcast infers element type correctly" begin
    v = [1, 2, 3]
    w = [4, 5, 6]
    fv = [1.0, 2.0, 3.0]

    # Vector .+ Vector -> Vector{Int64}
    @test typeof(v .+ w) == Vector{Int64}
    # Vector .* scalar -> Vector{Int64}
    @test typeof(v .* 2) == Vector{Int64}
    # Vector{Float64} unary broadcast
    @test typeof(sqrt.(fv)) == Vector{Float64}
    # abs.(vector)
    @test typeof(abs.(v)) == Vector{Int64}
end

# ===== source: type_inference/heterogeneous_array_runtime_union_3549.jl =====
# Issue #3549: heterogeneous array literals containing `nothing` (or `missing`)
# plus exactly one other concrete type should report the parametric Union
# element type from `typeof`/`eltype`, not collapse to `Any`.

@testset "Issue #3549 heterogeneous array Union element types" begin
    a = [1, nothing, 2]
    @test typeof(a) === Vector{Union{Nothing, Int64}}
    @test eltype(a) === Union{Nothing, Int64}

    b = [1.5, nothing, 2.5]
    @test typeof(b) === Vector{Union{Nothing, Float64}}
    @test eltype(b) === Union{Nothing, Float64}

    c = ["x", nothing, "y"]
    @test typeof(c) === Vector{Union{Nothing, String}}
    @test eltype(c) === Union{Nothing, String}

    d = [1, missing, 2]
    @test typeof(d) === Vector{Union{Missing, Int64}}
    @test eltype(d) === Union{Missing, Int64}

    # Homogeneous cases must remain unchanged.
    @test typeof([1, 2, 3]) === Vector{Int64}
    @test typeof([1.0, 2.0]) === Vector{Float64}
end

# ===== source: type_inference/heterogeneous_array_runtime_union_3way_3558.jl =====
# Issue #3558: heterogeneous array literals mixing 3+ types where the non-
# Nothing/Missing concretes are numeric should apply numeric promotion across
# the concretes and report the parametric Union element type, not collapse to
# `Any`. The 2-way case is covered by Issue #3549.

@testset "Issue #3558 heterogeneous 3-way Union with promotion" begin
    a = [1, nothing, 2.5]
    @test typeof(a) === Vector{Union{Nothing, Float64}}
    @test eltype(a) === Union{Nothing, Float64}

    b = [1.0, missing, 2]
    @test typeof(b) === Vector{Union{Missing, Float64}}
    @test eltype(b) === Union{Missing, Float64}

    c = [1, 2.0, missing, nothing]
    @test typeof(c) === Vector{Union{Missing, Nothing, Float64}}
    @test eltype(c) === Union{Missing, Nothing, Float64}

    # Both Missing and Nothing alongside a single Int promote naturally.
    d = [1, nothing, missing, 2]
    @test typeof(d) === Vector{Union{Missing, Nothing, Int64}}
    @test eltype(d) === Union{Missing, Nothing, Int64}
end

# ===== source: type_inference/hof_type_inference.jl =====
# Higher-order function type inference test
# Tests type inference for map and filter with lambda functions
#
# NOTE: Nested HOF calls have a known bug (Issue #1361).
# Tests 5 and 6 use intermediate variables as a workaround.


@testset "Higher-order function type inference" begin
    # Test 1: map with addition lambda (type preserved for same-type ops)
    result1 = map(x -> x + 1, [1, 2, 3])
    @test result1 == [2, 3, 4]
    @test length(result1) == 3

    # Test 2: map with multiplication lambda
    result2 = map(x -> x * 2, [1, 2, 3])
    @test result2 == [2, 4, 6]

    # Test 3: filter (type should be preserved)
    result3 = filter(x -> x > 0, [-1, 0, 1, 2, 3])
    @test result3 == [1, 2, 3]

    # Test 4: map with Float64 array
    result4 = map(x -> x + 0.5, [1.0, 2.0, 3.0])
    @test result4 == [1.5, 2.5, 3.5]

    # Test 4b: inline lambda return type feeds map result eltype inference
    result4b = map(x -> x * 2.0, [1, 2, 3])
    @test result4b == [2.0, 4.0, 6.0]
    @test typeof(result4b) === Vector{Float64}

    # Test 4c: qualified HOF calls use the same inline lambda return inference
    result4c = Base.map(x -> x * 2.0, [1, 2, 3])
    @test result4c == [2.0, 4.0, 6.0]
    @test typeof(result4c) === Vector{Float64}

    result4d = Base.broadcast(x -> x * 2.0, [1, 2, 3])
    @test result4d == [2.0, 4.0, 6.0]
    @test typeof(result4d) === Vector{Float64}

    # Test 5: nested map (Issue #1361 workaround: use intermediate variable)
    inner5 = map(x -> x + 1, [1, 2, 3])
    result5 = map(x -> x * 2, inner5)
    @test result5 == [4, 6, 8]

    # Test 6: chained filter and map (Issue #1361 workaround: use intermediate variable)
    filtered6 = filter(x -> x > 0, [-1, 0, 1, 2, 3])
    result6 = map(x -> x * 2, filtered6)
    @test result6 == [2, 4, 6]

    # Test 7: map with square function
    result7 = map(x -> x * x, [1, 2, 3, 4])
    @test result7 == [1, 4, 9, 16]

    # Test 8: inline lambda return type feeds reduce result inference
    result8 = reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])
    @test result8 == 3.5
    @test typeof(result8) === Float64

    # Test 9: qualified reduction HOF calls use the same inline lambda inference
    result9 = Base.reduce((acc, x) -> acc + x * 0.5, [1, 2, 3])
    @test result9 == 3.5
    @test typeof(result9) === Float64

    result9b = Base.mapreduce(x -> x * 0.5, +, [1, 2, 3])
    @test result9b == 3.0
    @test typeof(result9b) === Float64

    # Test 10: qualified reduction HOF init keyword calls use the positional rewrite
    @test Base.reduce(+, [1, 2, 3]; init = 10) == 16
    @test Base.foldl(+, [1, 2, 3]; init = 10) == 16
    @test Base.foldr(+, [1, 2, 3]; init = 10) == 16
    @test Base.mapreduce(identity, +, [1, 2, 3]; init = 10) == 16
    @test Base.mapfoldl(identity, +, [1, 2, 3]; init = 10) == 16
    @test Base.mapfoldr(identity, +, [1, 2, 3]; init = 10) == 16
end

# ===== source: type_inference/loop_element_inference.jl =====
# Loop element type inference test
# Tests that loop variables are correctly inferred from iterables


@testset "Loop element type inference" begin
    # Array iteration - element type should be inferred as Int64
    arr = [1, 2, 3]
    total = 0
    for x in arr
        total += x
    end
    @test total == 6

    # Float array iteration - element type should be Float64
    farr = [1.0, 2.0, 3.0]
    ftotal = 0.0
    for x in farr
        ftotal += x
    end
    @test ftotal == 6.0

    # Tuple iteration
    t = (1, 2, 3)
    sum_t = 0
    for x in t
        sum_t += x
    end
    @test sum_t == 6

    # Range iteration - element type should be Int
    sum_range = 0
    for i in 1:5
        sum_range += i
    end
    @test sum_range == 15

    # Nested loops
    result = 0
    for i in 1:3
        for j in [10, 20, 30]
            result += i * j
        end
    end
    @test result == 360  # (1+2+3) * (10+20+30)

    # String iteration - element type should be Char
    s = "abc"
    chars = Char[]
    for c in s
        push!(chars, c)
    end
    @test length(chars) == 3
    # Char == Char comparison is not yet supported (see issue #945)
    # Using Int conversion as workaround
    @test Int(chars[1]) == Int('a')
end

# ===== source: type_inference/map_element_type.jl =====
# Test that map infers the result element type from the mapped function (Issue #3480)


@testset "type_inference_map_element_type: map result element type" begin
    # map(Float64, Int array) -> Vector{Float64}
    result1 = map(Float64, [1, 2, 3])
    @test isa(result1, Array)
    @test result1[1] == 1.0
    @test result1[2] == 2.0

    # map(x -> x * 2, Int array) -> elements are Int
    result2 = map(x -> x * 2, [1, 2, 3])
    @test isa(result2, Array)
    @test result2[1] == 2
    @test result2[2] == 4
    @test result2[3] == 6

    # map returns correct values
    result3 = map(abs, [-1, -2, 3])
    @test result3[1] == 1
    @test result3[2] == 2
    @test result3[3] == 3
end

# ===== source: type_inference/namedtuple_getfield.jl =====
# Test NamedTuple getfield type inference (Issue #1638)
# getfield((a=1, b=2.0), :b) should infer as Float64


@testset "NamedTuple getfield inference" begin
    # Basic NamedTuple field access
    nt1 = (a=1, b=2.0)
    @test nt1.a == 1
    @test typeof(nt1.a) == Int64
    @test nt1.b == 2.0
    @test typeof(nt1.b) == Float64

    # Direct NamedTuple literal field access
    @test (x=10, y="hello").x == 10
    @test typeof((x=10, y="hello").x) == Int64
    @test (x=10, y="hello").y == "hello"
    @test typeof((x=10, y="hello").y) == String

    # Mixed type NamedTuple
    mixed = (flag=true, count=42, value=3.14, name="test")
    @test mixed.flag == true
    @test typeof(mixed.flag) == Bool
    @test mixed.count == 42
    @test typeof(mixed.count) == Int64
    @test mixed.value == 3.14
    @test typeof(mixed.value) == Float64
    @test mixed.name == "test"
    @test typeof(mixed.name) == String

    # getfield function call
    @test getfield(nt1, :a) == 1
    @test typeof(getfield(nt1, :a)) == Int64
    @test getfield(nt1, :b) == 2.0
    @test typeof(getfield(nt1, :b)) == Float64
end

# ===== source: type_inference/size_tuple_arity.jl =====
# Test size() returns tuple with correct arity for multidimensional arrays (Issue #3463)


@testset "type_inference_size_tuple_arity: size reflects array dimensions" begin
    v = [1.0, 2.0, 3.0]
    m = [1.0 2.0; 3.0 4.0]

    # 1D: size returns Tuple{Int64}
    @test typeof(size(v)) == Tuple{Int64}
    @test size(v) == (3,)

    # 2D: size returns Tuple{Int64, Int64}
    @test typeof(size(m)) == Tuple{Int64, Int64}
    @test size(m) == (2, 2)

    # size with dim index returns Int64
    @test typeof(size(m, 1)) == Int64
    @test typeof(size(m, 2)) == Int64
end

# ===== source: type_inference/struct_iterable_inference.jl =====
# Struct iterable type inference test
# Tests that loop variables are correctly inferred from struct iterables
# like LinRange, StepRangeLen, UnitRange, StepRange, and OneTo


@testset "Struct iterable type inference" begin
    # LinRange iteration - element type should be Float64
    lr = LinRange(0.0, 1.0, 5)
    sum_lr = 0.0
    for x in lr
        sum_lr += x
    end
    @test sum_lr == 2.5  # 0.0 + 0.25 + 0.5 + 0.75 + 1.0

    # Using collect to verify values
    lr_collected = collect(lr)
    @test length(lr_collected) == 5
    @test lr_collected[1] == 0.0
    @test lr_collected[5] == 1.0

    # StepRangeLen iteration via range() function
    # range(0, 1, length=5) creates a StepRangeLen
    srl = range(0.0, 1.0, length=5)
    sum_srl = 0.0
    for x in srl
        sum_srl += x
    end
    @test sum_srl == 2.5

    # Direct LinRange with integer bounds
    lr2 = LinRange(1, 10, 10)
    sum_lr2 = 0.0
    for x in lr2
        sum_lr2 += x
    end
    @test sum_lr2 == 55.0  # 1 + 2 + ... + 10 = 55

    # Nested loop with struct iterables
    total = 0.0
    for i in LinRange(1.0, 3.0, 3)
        for j in LinRange(1.0, 2.0, 2)
            total += i * j
        end
    end
    @test total == 18.0  # (1+2+3) * (1+2) = 6 * 3 = 18

    # Enumerate with LinRange
    indices = Int64[]
    values = Float64[]
    for (i, v) in enumerate(LinRange(0.0, 2.0, 3))
        push!(indices, i)
        push!(values, v)
    end
    @test length(indices) == 3
    @test indices[1] == 1
    @test indices[3] == 3
    @test values[1] == 0.0
    @test values[3] == 2.0
end

# ===== source: type_inference/tuple_const_index.jl =====
# Test Tuple constant index type inference (Issue #1638)
# (1, 2.0)[1] should infer as Int64, not a generic tuple element type


@testset "Tuple constant index inference" begin
    # Basic tuple indexing with constant index
    t1 = (1, 2.0, "hello")
    @test t1[1] == 1
    @test typeof(t1[1]) == Int64
    @test t1[2] == 2.0
    @test typeof(t1[2]) == Float64
    @test t1[3] == "hello"
    @test typeof(t1[3]) == String

    # Direct tuple literal indexing
    @test (10, 20.5)[1] == 10
    @test typeof((10, 20.5)[1]) == Int64
    @test (10, 20.5)[2] == 20.5
    @test typeof((10, 20.5)[2]) == Float64

    # Mixed type tuple
    mixed = (true, 42, 3.14, "test")
    @test mixed[1] == true
    @test typeof(mixed[1]) == Bool
    @test mixed[2] == 42
    @test typeof(mixed[2]) == Int64
    @test mixed[3] == 3.14
    @test typeof(mixed[3]) == Float64
    @test mixed[4] == "test"
    @test typeof(mixed[4]) == String
end

# ===== source: type_inference/typed_empty_array_int128_3557.jl =====
# Issue #3557: typed empty array literals `Int128[]` and `UInt128[]` should
# preserve their declared element type at runtime, even though the underlying
# storage is the boxed `Vec<Value>` path. The 64-bit-and-smaller types are
# already covered by Issue #3548.

@testset "Issue #3557 Int128/UInt128 typed empty arrays" begin
    a = Int128[]
    @test typeof(a) === Vector{Int128}
    @test eltype(a) === Int128
    @test isempty(a)

    b = UInt128[]
    @test typeof(b) === Vector{UInt128}
    @test eltype(b) === UInt128
    @test isempty(b)

    # Push! preserves element type
    push!(a, Int128(1))
    push!(a, Int128(2))
    @test typeof(a) === Vector{Int128}
    @test length(a) == 2
    @test a[1] === Int128(1)
    @test a[2] === Int128(2)

    push!(b, UInt128(1))
    push!(b, UInt128(2))
    @test typeof(b) === Vector{UInt128}
    @test length(b) == 2
    @test b[1] === UInt128(1)
    @test b[2] === UInt128(2)
end

# ===== source: type_inference/typed_empty_array_runtime_types_3548.jl =====
# Issue #3548: typed empty array literals (Int32[], Float32[], UInt8[], …)
# must report `Vector{T}` from `typeof` at runtime, not `Vector{Int64}` /
# `Vector{Float64}`.

@testset "Issue #3548 typed empty array runtime types" begin
    @test typeof(Int32[]) === Vector{Int32}
    @test typeof(Int16[]) === Vector{Int16}
    @test typeof(Int8[]) === Vector{Int8}
    @test typeof(UInt8[]) === Vector{UInt8}
    @test typeof(UInt16[]) === Vector{UInt16}
    @test typeof(UInt32[]) === Vector{UInt32}
    @test typeof(UInt64[]) === Vector{UInt64}
    @test typeof(Float32[]) === Vector{Float32}
    @test typeof(Float64[]) === Vector{Float64}
    @test typeof(Bool[]) === Vector{Bool}
    @test typeof(Int64[]) === Vector{Int64}

    # Pushing values of the correct type works and preserves element type.
    xs = Int32[]
    push!(xs, Int32(7))
    @test eltype(xs) === Int32
    @test xs[1] === Int32(7)

    ys = UInt8[]
    push!(ys, 0x05)
    @test eltype(ys) === UInt8
    @test ys[1] === UInt8(5)
end

# ===== source: type_inference/typed_range_runtime_element_3550.jl =====
# Issue #3550: ranges constructed from typed integers must preserve their
# declared element type. Both `typeof(range)` and the loop variable must
# reflect the operand type (`UInt8`, `Int32`, …) instead of widening to
# `UnitRange{Int64}` / `Int64`.

@testset "Issue #3550 typed range element types" begin
    r_u8 = UInt8(1):UInt8(3)
    @test typeof(r_u8) === UnitRange{UInt8}
    @test first(r_u8) === UInt8(1)
    @test last(r_u8) === UInt8(3)

    seen_u8 = UInt8[]
    for x in r_u8
        @test typeof(x) === UInt8
        push!(seen_u8, x)
    end
    @test length(seen_u8) == 3
    @test seen_u8[1] === UInt8(1)
    @test seen_u8[2] === UInt8(2)
    @test seen_u8[3] === UInt8(3)

    # Inline range form must also preserve the operand type.
    saw = false
    for x in UInt8(1):UInt8(2)
        @test typeof(x) === UInt8
        saw = true
    end
    @test saw

    # Int32 range
    r_i32 = Int32(1):Int32(3)
    @test typeof(r_i32) === UnitRange{Int32}

    # Plain integer ranges still default to Int64.
    @test typeof(1:3) === UnitRange{Int64}
    @test first(1:3) === 1
end

true
