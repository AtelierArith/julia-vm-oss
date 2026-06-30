# Issue #4927: `:0xFF` evaluated to `Int64(255)` instead of
# `UInt8(0xFF)`. Bare `0xFF` correctly produced `UInt8`, so the type
# information was lost specifically on the colon-prefix-of-literal
# path added in PR #4926 (Issue #4923).
#
# Root cause: the `cst_to_expr_constructor::IntegerLiteral` arm used
# `parse_int` (untyped), which discarded the hex / binary / octal
# width tag. The bare-literal lowering elsewhere uses
# `lower_integer_literal` (`parse_int_typed` + wrap in
# `UInt8 / UInt16 / …` constructor when the kind is set).
#
# Fix: in `lowering/expr/quote/cst_to_constructor.rs`, route the
# `NodeKind::IntegerLiteral` arm through the same
# `lower_integer_literal` helper. `lower_integer_literal` is now
# `pub(super)` so the quote module can reuse it.

using Test

@testset "colon-prefix hex literal preserves UInt type (Issue #4927)" begin
    @test :0xFF === 0xFF
    @test typeof(:0xFF) === UInt8

    @test :0x100 === 0x100
    @test typeof(:0x100) === UInt16

    @test :0x10000 === 0x10000
    @test typeof(:0x10000) === UInt32
end

@testset "colon-prefix binary literal preserves UInt type (Issue #4927)" begin
    @test :0b1010 === 0b1010
    @test typeof(:0b1010) === UInt8

    @test :0b1_0000_0000 === 0b1_0000_0000
    @test typeof(:0b1_0000_0000) === UInt16
end

@testset "decimal :literal stays Int64 (regression guard, Issue #4927)" begin
    @test :42 === 42
    @test typeof(:42) === Int64
    @test :0 === 0
end

true
