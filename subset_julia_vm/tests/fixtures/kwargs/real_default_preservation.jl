using Test

# Regression test for Issue #3623:
# `function f(; x::Real = 0.1) ... end; f()` was returning `0::Int64`
# instead of `0.1::Float64`. Same root cause as #3653 (typed-kwarg
# default lowering treated the bare-Identifier type as the default
# value); fixed by the parse_kwparam_from_kw_node patch in #3686.
#
# Discovered in #3501 (timedwait): `pollint::Real=0.1` lost the default,
# requiring a runtime-validation workaround. With #3623 fixed, the
# annotation can be restored — see `subset_julia_vm/src/julia/base/task.jl`.

@testset "kwarg ::Real=0.1 preserves Float64 default (#3623)" begin
    function f(; x::Real = 0.1)
        return x
    end
    @test f() == 0.1
    @test typeof(f()) === Float64
end

@testset "kwarg ::Real=42 preserves Int64 default" begin
    function g(; x::Real = 42)
        return x
    end
    @test g() == 42
    @test typeof(g()) === Int64
end

@testset "kwarg ::Number=1.5 preserves Float64 default" begin
    function h(; x::Number = 1.5)
        return x
    end
    @test h() == 1.5
    @test typeof(h()) === Float64
end

@testset "kwarg ::Integer=7 preserves Int64 default" begin
    function i(; x::Integer = 7)
        return x
    end
    @test i() == 7
    @test typeof(i()) === Int64
end

@testset "timedwait pollint::Real=0.1 default reachable (#3501)" begin
    # Sanity check that the Issue #3501 workaround is no longer needed.
    # We don't actually wait — we pass a predicate that's already true.
    @test timedwait(() -> true, 0.0) === :ok
end

true
