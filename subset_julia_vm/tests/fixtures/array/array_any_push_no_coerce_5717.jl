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

true
