# Issue #4777: string(::NamedTuple) and string(::Tuple) recursed into
# field/element values via the print form (bare), so String/Char
# values lost their quotes. Sibling of the already-fixed #4764
# (struct field show form) — same root cause family but for the
# NamedTuple- and Tuple-specific format arms in format_value_slow.
#
# Fix: NamedTuple and Tuple value mapping in
# subset_julia_vm_vm/src/vm/formatting.rs now uses
# format_value_show_field, which quotes Value::Str and Value::Char.
# As part of the same change, single-element Tuple format now emits
# the trailing comma `(1,)` to match upstream's disambiguation form.

using Test

@testset "string(::NamedTuple) quotes String/Char fields (Issue #4777)" begin
    nt = (x = 1, y = "hi", z = 'c')
    @test string(nt) == "(x = 1, y = \"hi\", z = 'c')"

    buf = IOBuffer()
    print(buf, nt)
    @test String(take!(buf)) == "(x = 1, y = \"hi\", z = 'c')"
end

@testset "string(::Tuple) quotes String/Char elements (Issue #4777)" begin
    t = (1, "hi", 'c')
    @test string(t) == "(1, \"hi\", 'c')"

    buf = IOBuffer()
    print(buf, t)
    @test String(take!(buf)) == "(1, \"hi\", 'c')"
end

@testset "string(::Tuple) single-element gets trailing comma (Issue #4777)" begin
    @test string((1,)) == "(1,)"
    @test string(("hi",)) == "(\"hi\",)"
end

@testset "string(::Tuple) empty tuple has no spurious comma (Issue #4777)" begin
    @test string(()) == "()"
end

@testset "string(::Tuple) numeric-only unchanged (Issue #4777)" begin
    # Regression guard: pure-numeric tuples must keep their existing form
    @test string((1, 2, 3)) == "(1, 2, 3)"
end

true
