using Test

# Block (function … end) form with bare-identifier optional defaults.
# Regression for Issue #8017: a default value that is an identifier/global
# reference (nothing, missing, a const, a type) was silently dropped, so the
# default-arg stub method was never generated and the reduced-arity call
# failed to dispatch.

const GLOBAL_DEFAULT = 99

module M
function f(x, y, st::Symbol, a=:auto, t="", l=nothing)
    return (x, y, st, a, t, l)
end
g(x, y) = f(x, y, :line, :auto, "T", nothing)
end

function single_nothing(x, l=nothing)
    return l
end

function single_missing(x, m=missing)
    return m
end

function ident_default(x, l=GLOBAL_DEFAULT)
    return l
end

function type_default(x, T=Int)
    return T
end

@testset "block-form identifier defaults (Issue #8017)" begin
    # The issue's exact MWE: intra-module call relying on a function-form stub.
    @test M.g(1, 2) == (1, 2, :line, :auto, "T", nothing)
    # Reduced-arity calls that rely on the generated stubs.
    @test M.f(1, 2, :line) == (1, 2, :line, :auto, "", nothing)
    @test M.f(1, 2, :line, :solid) == (1, 2, :line, :solid, "", nothing)
    @test M.f(1, 2, :line, :solid, "Title") == (1, 2, :line, :solid, "Title", nothing)
    @test M.f(1, 2, :line, :solid, "Title", :lbl) == (1, 2, :line, :solid, "Title", :lbl)

    # Minimal single-optional cases with bare-identifier defaults.
    @test single_nothing(1) === nothing
    @test single_nothing(1, 5) == 5
    @test single_missing(1) === missing
    @test ident_default(1) == 99
    @test ident_default(1, 7) == 7
    @test type_default(1) === Int
end

true
