# Regression for Issue #4376: a `::Bool` return-type annotation must not
# erase the static return type to `Int64`. Before the fix, the compiler
# mapped `JuliaType::Bool` to `ValueType::I64` in `julia_type_to_value_type`,
# so a `function f()::Bool; true; end` caller saw `f()` as Int64, and
# multiple dispatch routed Bool-typed call sites (println/show, and any
# user-defined `foo(::Bool)` vs `foo(::Int64)`) to the Int64 method —
# making `println(isprime(17))` print `1` instead of `true`.

using Test

foo(x::Bool) = "bool"
foo(x::Int64) = "int"

function fbool_annotated()::Bool
    return true
end

function ffalse_annotated()::Bool
    return false
end

function fbool_noann()
    return true
end

@testset "Bool return-type annotation preserves Bool dispatch" begin
    # Static dispatch on Bool-typed return must reach foo(::Bool),
    # not foo(::Int64). This is the exact path that breaks println.
    @test foo(fbool_annotated()) == "bool"
    @test foo(ffalse_annotated()) == "bool"

    # Sanity: bare literal and no-annotation cases already worked.
    @test foo(true) == "bool"
    @test foo(fbool_noann()) == "bool"
    @test foo(1) == "int"

    # Runtime Value is preserved (already worked before the fix).
    @test typeof(fbool_annotated()) == Bool
    @test fbool_annotated() === true
    @test ffalse_annotated() === false
end

# Fixture exit value used by the test runner. Each clause has to hold;
# `&&` short-circuits and turns any failure into a `false` exit so the
# fixture runner reports the regression even outside the @testset.
foo(fbool_annotated()) == "bool" &&
    foo(ffalse_annotated()) == "bool" &&
    foo(true) == "bool" &&
    foo(1) == "int"
