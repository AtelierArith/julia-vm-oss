# A user-defined custom Unicode operator accepted in DEFINITION position must
# also be accepted as an ordinary CALL target, dispatching through the same
# method-table path as named functions (Issue #10933). The call `⊗(1)` used to
# be rejected during lowering as UnsupportedCallTarget even though the
# definition `⊗(x::T) where T = T` lowered fine.
#
# Root cause: `resolve_call_target` (lowering/expr/call.rs) only allowed an
# allowlist of ASCII operators (`is_operator_function_call_target`) in call
# position and rejected every other Operator token. The fix inverts the final
# arm: any operator that is not a syntactic form (`&&`, `||`, `<:`, `>:`,
# assignments, `->`, `...`, `::`, `?`, `=>`) is an ordinary function name.
#
# NOTE: INFIX use of a custom Unicode operator (`1 ⊗ 2`) is a separate gap in
# the binary-expression lowering path (`map_binary_op` fallback), tracked by
# Issue #11023. This fixture covers call-position only.
#
# All expectations verified against upstream Julia 1.12.

using Test

# --- MWE reproduction (Issue #10933): where-typed single-argument definition,
# then call-position use with dispatch on the argument type.
⊗(x::T) where T = T

@testset "custom unicode operator call target (Issue #10933)" begin
    @test ⊗(1) === Int64
    @test ⊗(1.0) === Float64
end

# --- Multiple methods and multi-argument calls dispatch like named functions.
⊕(a::Int64, b::Int64) = a + b
⊕(a::Float64, b::Float64) = a * b
⊕(a, b, c) = "three"

@testset "custom unicode operator multi-method dispatch" begin
    @test ⊕(2, 3) == 5
    @test ⊕(2.0, 3.0) == 6.0
    @test ⊕(1, 2, 3) == "three"
end

# --- Splatted call-position arguments.
@testset "custom unicode operator splat call" begin
    xs = [4, 5]
    @test ⊕(xs...) == 9
end

# --- Base Unicode comparison aliases are also plain functions in call
# position (they were rejected by the old allowlist too).
@testset "base unicode comparison operators in call position" begin
    @test ≥(3, 2)
    @test !≤(3, 2)
end

# --- Issue #11023: the INFIX spelling of the same user-defined operator.
# Upstream Julia has no separate "operator" namespace: every non-syntactic
# operator is an ordinary function name, so a custom operator may be used infix
# exactly as it was defined, sharing the prefix form's method identity.
# (Issue #11083 later broadened the glyph set: the lexer now derives its
# operator characters from upstream's precedence tables, so the section below
# covers ⊛/⊞/⊠/⋆, suffixed names such as ⊗ᵢ, and per-class precedence.)
@testset "custom unicode operators infix (Issue #11023)" begin
    # A two-argument method on the operator already defined above.
    ⊗(a::Int64, b::Int64) = a * b + 1
    @test 1 ⊗ 2 == 3
    @test ⊗(1, 2) == 3            # prefix and infix agree
    @test (1 ⊗ 2) == ⊗(1, 2)

    # Dispatch works through the infix spelling.
    ⊘(a::Int64, b::Int64) = "ints"
    ⊘(a::Float64, b::Float64) = "floats"
    @test 1 ⊘ 2 == "ints"
    @test 1.0 ⊘ 2.0 == "floats"

    # Base Unicode operators keep working infix.
    @test 3 ≥ 2
    @test 2 ≤ 3
end

# --- Issue #11083: the operator CHARACTER SET is derived from upstream's
# precedence tables (`julia/src/julia-parser.scm`), not an ad-hoc allowlist, so
# every character upstream calls an operator lexes as one — and an operator name
# may carry upstream's operator suffixes (primes, sub/superscripts, combining
# marks; `jl_op_suffix_char`). All expectations verified against upstream Julia
# 1.12.6.
⊛(a::Int64, b::Int64) = a * b
⊠(a::Int64, b::Int64) = a * b + 100
⋆(a::Int64, b::Int64) = a - b
⊞(a::Int64, b::Int64) = a + b
⊗ᵢ(a::Int64, b::Int64) = a * b + 1

@testset "unicode operator glyph set (Issue #11083)" begin
    # Previously unlexable glyphs, prefix and infix.
    @test 3 ⊛ 4 == 12
    @test ⊛(3, 4) == 12
    @test 3 ⊠ 4 == 112
    @test 7 ⋆ 4 == 3
    @test 2 ⊞ 5 == 7

    # An operator name carrying a subscript suffix.
    @test ⊗ᵢ(1, 2) == 3
    @test 1 ⊗ᵢ 2 == 3
end

@testset "unicode operator precedence follows upstream class (Issue #11083)" begin
    # ⊛/⊠/⋆/⊗ᵢ are prec-times, ⊞ is prec-plus: times binds tighter than plus,
    # and same-class operators are left-associative.
    @test 1 ⊛ 2 + 3 == 5          # (1 ⊛ 2) + 3
    @test 1 + 2 ⊛ 3 == 7          # 1 + (2 ⊛ 3)
    @test 2 ⊞ 3 ⊛ 4 == 14         # 2 ⊞ (3 ⊛ 4)
    @test 2 ⊛ 3 ⊗ᵢ 4 == 25        # (2 ⊛ 3) ⊗ᵢ 4 — left-assoc within prec-times
    @test 2 ⊛ 3 * 2 == 12         # mixes with the ASCII times operator
end

@testset "unicode operator broadcast keeps its class (Issue #11110)" begin
    xs = [1, 2]
    ys = [3, 4]
    # `.⊛` is times-precedence like `.*`, so it binds tighter than `.+`.
    @test xs .⊛ ys .+ 1 == [4, 9]
    @test xs .⊛ ys == [3, 8]
end

# Non-operator math symbols are still ordinary identifiers (upstream rejects
# `⊝` as an unknown character, and `∞` / `∇` are identifier characters).
∇(x) = x + 1

@testset "non-operator math symbols stay identifiers (Issue #11083)" begin
    @test ∇(1) == 2
end

true
