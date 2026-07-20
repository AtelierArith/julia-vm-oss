# Literal type application `T{...}` on a bare non-parametric base
# (`Int64{Float64}`, `Real{Int64}`) raises TypeError like upstream
# jl_apply_type (which requires a UnionAll base) instead of silently
# fabricating a nonsense DataType. The `Core.apply_type` spelling of the same
# application was fixed by #10587/#10554; this covers the literal-brace
# compile path, which now routes through the same runtime ApplyTypeDynamic
# validator. Issue #10654 (residual leg of the #10556 parity matrix).

using Test

struct NonParam10654 end
struct Pair10654{A,B} end

apply_any_10654() = Any{Int64}
apply_str_10654() = String{Int64}

@testset "literal T{...} on a non-parametric base raises TypeError (Issue #10654)" begin
    # MWE: bare concrete and abstract builtin bases.
    err = try
        Int64{Float64}
        nothing
    catch e
        e
    end
    @test typeof(err) == TypeError

    @test_throws TypeError Int64{Float64}
    @test_throws TypeError Real{Int64}
    @test_throws TypeError Any{Int64}
    @test_throws TypeError String{Int64}
    @test_throws TypeError Bool{Int64}
    @test_throws TypeError Nothing{Int64}
    @test_throws TypeError Float64{Int64}
    @test_throws TypeError Signed{Int64}

    # A non-parametric user struct base raises the same TypeError.
    @test_throws TypeError NonParam10654{Int64}

    # Inside a function body the literal takes the same path.
    @test_throws TypeError apply_any_10654()
    @test_throws TypeError apply_str_10654()

    # Literal and Core.apply_type spellings agree (#10587 parity).
    @test_throws TypeError Core.apply_type(Int64, Float64)
    @test_throws TypeError Core.apply_type(Real, Int64)

    # Positive controls: parametric families still apply statically.
    @test Vector{Int64} === Vector{Int64}
    @test Pair10654{Int64,Float64} === Pair10654{Int64,Float64}
    @test Union{Int64,Float64} == Union{Float64,Int64}
    @test NTuple{2,Int64} === Tuple{Int64,Int64}
    @test Val{5} === Val{5}
end

true
