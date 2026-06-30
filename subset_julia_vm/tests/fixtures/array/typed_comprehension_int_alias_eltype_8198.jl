# Issue #8198: typed comprehension `Int[expr for ...]` lost its element type and
# produced a `Vector{Any}` instead of `Vector{Int}`.
#
# Root cause was in the *inference* path (not the runtime values, which were
# always correct): the ValueType type-function table that resolves a numeric
# constructor call (`Int(x)`) to a concrete element type had every fixed-width
# name (Int8…Int128, Float16…Float64) but was missing the platform-native word
# aliases `Int` / `UInt`. So `Int(i^2)` inferred `Any`, the comprehension was
# allocated as `Vector{Any}`, and `eltype` disagreed with upstream. The
# fixed-width `Int32[...]` already worked, which is why only the `Int`/`UInt`
# aliases regressed.
#
# Final value is the logical AND of all checks so the nextest harness actually
# validates them (a bare `@testset` ending in `true` would be false-green until
# Issue #8191 is fixed).

using Test

checks = Bool[]
chk(c) = (push!(checks, c); c)

@testset "typed comprehension Int/UInt alias eltype (Issue #8198)" begin
    # ---- the MWE: Int[...] keeps Int64 eltype and the right values ----
    a = Int[i^2 for i in 1:3]
    @test chk(a == [1, 4, 9])
    @test chk(eltype(a) == Int64)
    @test chk(typeof(a) == Vector{Int64})

    # bare-var, scaled, and shifted bodies all keep Int64
    @test chk(eltype(Int[i for i in 1:3]) == Int64)
    @test chk(eltype(Int[2i for i in 1:3]) == Int64)
    @test chk(eltype(Int[i + 1 for i in 1:3]) == Int64)

    # UInt alias likewise resolves to the native unsigned word type
    @test chk(eltype(UInt[i for i in 1:3]) == UInt64)
    @test chk(eltype(UInt[i^2 for i in 1:3]) == UInt64)

    # Fixed-width names keep working (regression guard)
    @test chk(eltype(Int32[i^2 for i in 1:3]) == Int32)
    @test chk(eltype(Int8[i for i in 1:3]) == Int8)
    @test chk(eltype(Float64[i for i in 1:3]) == Float64)

    # Filtered and multi-iterator (2-D) typed comprehensions also keep Int64
    @test chk(eltype(Int[i for i in 1:6 if iseven(i)]) == Int64)
    @test chk(Int[i for i in 1:6 if iseven(i)] == [2, 4, 6])
    m = Int[i * j for i in 1:2, j in 1:2]
    @test chk(eltype(m) == Int64)
    @test chk(m == [1 2; 2 4])

    # Scalar Int(...) construction in a plain array literal keeps the eltype too
    @test chk(eltype([Int(5)]) == Int64)
    @test chk(typeof(Int(5)) == Int64)
end

all(checks)
