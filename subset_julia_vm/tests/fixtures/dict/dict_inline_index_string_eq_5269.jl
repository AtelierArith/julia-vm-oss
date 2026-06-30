# Inline `dict[key]` returning a String must compare equal to a String with `==`
# (Issue #5269).
#
# Root cause: the compiler inferred the result type of `dict[key]` (an inline
# `Expr::Index` on a `ValueType::Dict` local) as `Int64` unconditionally. When
# the actual value was a `String`, the `String vs non-String` constant-fold
# shortcut in `==` compilation folded the comparison to `false`. Inference must
# treat untyped-Dict element access as runtime-unknown (`Any`).

using Test

@testset "Dict inline string-value indexing == (Issue #5269)" begin
    # Original repro from the issue
    ds = Dict("a" => "x")
    @test ds["a"] == "x"
    @test "x" == ds["a"]
    @test !(ds["a"] == "y")
    @test ds["a"] != "y"

    # Int key, String value
    dk = Dict(1 => "x")
    @test dk[1] == "x"

    # Float64 key, String value
    df = Dict(1.0 => "x")
    @test df[1.0] == "x"

    # Inline index compared against a String variable
    z = "x"
    @test ds["a"] == z

    # Int-valued dicts must still work (no regression)
    di = Dict("a" => 1)
    @test di["a"] == 1
    @test di["a"] + 1 == 2

    # Binding to a local first already worked; keep as a guard
    r = ds["a"]
    @test r == "x"
end

true
