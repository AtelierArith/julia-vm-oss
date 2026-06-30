using Test

# Regression test for Issue #3653:
# Keyword arguments with explicit type annotations like `x::Bool=true`,
# `x::Int64=42`, `x::Float64=1.5`, `x::String="hi"` previously lowered
# the default to `Int64(0)` instead of the actual literal. For
# `x::Bool=true` this flipped a truthy default into a falsy one,
# breaking `if x` checks (discovered in #3651).
#
# The bug was in `parse_kwparam_from_kw_node`
# (lowering/function/signature.rs): when the type expression was a
# bare `Identifier` (e.g. `Bool`/`Int64`/`Float64`/`String`), the
# parser child list looked like `[Identifier name, Identifier type,
# default_value]`. The lowering code treated the second `Identifier`
# (the type) as the default and ignored the actual default literal.
#
# Fix: skip exactly one type-Identifier when `::` is present in the
# node text before treating the next child as the default.
#
@testset "kwarg ::Bool=true preserves truthy default (#3653)" begin
    function kw_bool_true_default_3653(; x::Bool=true)
        return x
    end
    @test kw_bool_true_default_3653() == true
    @test typeof(kw_bool_true_default_3653()) === Bool
end

@testset "kwarg ::Bool=false preserves falsy default" begin
    function kw_bool_false_default_3653(; x::Bool=false)
        return x
    end
    @test kw_bool_false_default_3653() == false
    @test typeof(kw_bool_false_default_3653()) === Bool
end

@testset "kwarg ::Int64=42 preserves Int64 default" begin
    function h(; x::Int64=42)
        return x
    end
    @test h() == 42
    @test typeof(h()) === Int64
end

@testset "kwarg ::Float64=1.5 preserves Float64 default" begin
    function i(; x::Float64=1.5)
        return x
    end
    @test i() == 1.5
    @test typeof(i()) === Float64
end

@testset "kwarg ::String=hi preserves String default" begin
    function j(; x::String="hi")
        return x
    end
    @test j() == "hi"
    @test typeof(j()) === String
end

@testset "kwarg ::Bool=true gives a truthy comparison" begin
    # Acceptance criterion from #3653: the default must be truthy
    # (matching `true` semantics), not falsy as it was before.
    function check(; flag::Bool=true)
        if flag
            return :was_true
        else
            return :was_false
        end
    end
    @test check() === :was_true
    @test check(flag=false) === :was_false
end

@testset "explicit kwarg overrides default" begin
    function kw_bool_override_default_3653(; x::Bool=true)
        return x
    end
    @test kw_bool_override_default_3653(x=false) == false
end

true
