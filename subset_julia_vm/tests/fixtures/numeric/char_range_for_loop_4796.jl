# Issue #4796 (follow-up to #4795): `for c in 'a':'c'` bound the loop
# variable as Int64 codepoints (97, 98, 99) instead of Char values.
# The first slice of #4795 fixed Range[i] / collect / first / last
# for Char ranges, but the for-loop iteration path used the
# I64-specialized fast path in `compile/stmt.rs::Stmt::For` which
# bypassed `RangeValue::typed_element`.
#
# Fix: include `ValueType::Char` in the `needs_typed_range` check
# that rewrites for-Range to ForEach over `Expr::Range` — same
# treatment small-int ranges (Int8/Int16/...) already got for the
# same reason in #3550.

using Test

@testset "for c in 'a':'c' binds Char (Issue #4796)" begin
    chars = Char[]
    for c in 'a':'c'
        push!(chars, c)
    end
    @test chars == ['a', 'b', 'c']
    @test eltype(chars) === Char
end

@testset "for c in 'a':'c' loop var typeof is Char (Issue #4796)" begin
    types = []
    for c in 'a':'c'
        push!(types, typeof(c))
    end
    @test all(t -> t === Char, types)
end

@testset "for c in 'e':-1:'a' reverse Char step range (Issue #4796)" begin
    chars = Char[]
    for c in 'e':-1:'a'
        push!(chars, c)
    end
    @test chars == ['e', 'd', 'c', 'b', 'a']
end

@testset "for c in 'x':'x' single-element Char range (Issue #4796)" begin
    chars = Char[]
    for c in 'x':'x'
        push!(chars, c)
    end
    @test chars == ['x']
end

@testset "for c in 'z':'a' empty Char range (Issue #4796)" begin
    chars = Char[]
    for c in 'z':'a'
        push!(chars, c)
    end
    @test isempty(chars)
end

@testset "for c in stored Char range variable (Issue #4796)" begin
    r = 'a':'c'
    chars = Char[]
    for c in r
        push!(chars, c)
    end
    @test chars == ['a', 'b', 'c']
end

@testset "for i in 1:3 Int regression — fast path preserved (Issue #4796)" begin
    # The fix should not regress the I64 fast path.
    s = 0
    for i in 1:3
        s += i
    end
    @test s == 6
end

true
