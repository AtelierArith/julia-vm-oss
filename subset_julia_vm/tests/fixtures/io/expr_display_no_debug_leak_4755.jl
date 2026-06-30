using Test

# Issue #4755: regression guard for `ExprValue` Display leaking Rust
# `Debug` repr (e.g. `I128(...)`) into user-visible output. After
# PR #4754 (Issue #4753) integer literal overflow promoted the
# magnitude `9223372036854775808` to Value::I128, which then leaked
# into `Expr` display as `Expr(:call, :-, I128(...))`.
#
# This fixture is sjulia-specific: the test asserts that the leaky
# `I128(...)` / `I32(...)` / etc. tokens do NOT appear in the
# string form of Expr values. Upstream Julia's Expr display uses a
# different form (`:(expr)` quoted syntax) which is tracked separately.

function expr_str_has_no_debug_leak(s::String)
    # Things that would indicate a Rust Debug repr leak.
    bad_tokens = ["I128(", "I32(", "I16(", "I8(",
                  "U128(", "U64(", "U32(", "U16(", "U8(",
                  "F32(", "F16(", "Str(", "Char("]
    for tok in bad_tokens
        if occursin(tok, s)
            return false
        end
    end
    return true
end

@testset "Expr display does not leak Rust Debug for I128 children (Issue #4755)" begin
    # The I128 magnitude comes from parsing -typemin(Int64) (PR #4754).
    e = Meta.parse("-9223372036854775808")
    s = string(e)
    @test expr_str_has_no_debug_leak(s)
    # The magnitude must still appear as a bare integer.
    @test occursin("9223372036854775808", s)
end

@testset "Expr display: typical arg types render cleanly (Issue #4755)" begin
    for src in ["x + 1", "x * 2", "f(:a, :b)", "g(\"hi\")"]
        s = string(Meta.parse(src))
        @test expr_str_has_no_debug_leak(s)
    end
end

true
