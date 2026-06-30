using Test

# Issue #4970: throwing math helpers must report their documented inferred
# exception type and a non-`nothrow` effect, instead of the blanket proven-total
# (Union{} / all-true) fallback. Values verified against upstream Julia 1.12.
#
# Issue #4986: Method reflection retains the `purity` field populated from
# `@assume_effects` metadata; the dedicated probe locks parity (already resolved
# upstream by commit ca49737c, fixture added here to guard against regression).

@testset "reflection infer_exception_type math helpers (#4970)" begin
    @test Base.infer_exception_type(sin, Tuple{Float64}) === DomainError
    # cos / sqrt over Float64 also throw DomainError (extended #4970).
    @test Base.infer_exception_type(cos, Tuple{Float64}) === DomainError
    @test Base.infer_exception_type(sqrt, Tuple{Float64}) === DomainError
    @test Base.infer_exception_type(log1p, Tuple{Float64}) === Union{DomainError,InexactError}
    # log over Float64 has the same Union as log1p (extended #4970).
    @test Base.infer_exception_type(log, Tuple{Float64}) === Union{DomainError,InexactError}
    @test Base.infer_exception_type(divrem, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(gcd, Tuple{Int64,Int64}) === OverflowError
    @test Base.infer_exception_type(lcm, Tuple{Int64,Int64}) === Union{DivideError,OverflowError}

    # exp is proven total -> no exception.
    @test Base.infer_exception_type(exp, Tuple{Float64}) === Union{}
end

@testset "reflection infer_effects math helpers nothrow (#4970)" begin
    # Throwing math helpers are total except for `nothrow`, which is false.
    for f in (sin, cos, sqrt, log, log1p)
        e = Base.infer_effects(f, Tuple{Float64})
        @test e.nothrow === false
        @test string(e) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    end
    for f in (divrem, gcd, lcm)
        e = Base.infer_effects(f, Tuple{Int64,Int64})
        @test e.nothrow === false
        @test string(e) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    end

    # exp stays proven-total.
    ee = Base.infer_effects(exp, Tuple{Float64})
    @test ee.nothrow === true
    @test string(ee) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end

@testset "reflection Method.purity retained (#4986)" begin
    reflection_plain_4986(x) = x + 1
    m = first(methods(reflection_plain_4986))
    @test m.purity === UInt16(0)
    @test typeof(m.purity) === UInt16
end

true
