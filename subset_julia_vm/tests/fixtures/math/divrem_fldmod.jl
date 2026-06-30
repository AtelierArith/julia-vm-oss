# Test divrem(), fldmod(), mod1(), fld1(), fldmod1() functions (Issue #1861)

using Test

# divrem(x, y) = (div(x, y), rem(x, y)); div rounds toward zero, rem keeps the
# sign of the dividend (Issue #6891 / #6895). For Int args the typed divrem
# returns Int components.
@testset "divrem basic" begin
    @test divrem(7, 3) == (2, 1)
    @test divrem(10, 5) == (2, 0)
    @test divrem(-7, 3) == (-2, -1)
end

@testset "fldmod basic" begin
    @test fldmod(7, 3) == (2, 1)
    @test fldmod(10, 5) == (2, 0)
end

@testset "mod1 basic" begin
    @test mod1(1, 3) == 1
    @test mod1(3, 3) == 3
    @test mod1(4, 3) == 1
    @test mod1(6, 3) == 3
end

@testset "fld1 basic" begin
    @test fld1(1, 3) == 1
    @test fld1(3, 3) == 1
    @test fld1(4, 3) == 2
end

@testset "fldmod1 basic" begin
    @test fldmod1(1, 3) == (1, 1)
    @test fldmod1(4, 3) == (2, 1)
    @test fldmod1(6, 3) == (2, 3)
end

# Real regression gate: the fixture harness only inspects the final value, so a
# trailing `true` would mask the @test failures above. Re-check the corrected
# results as an AND'd boolean so a wrong (non-throwing) value fails the fixture.
ok = true
ok = ok && (divrem(-7, 3) == (-2, -1))
ok = ok && (divrem(7, 3) == (2, 1))
ok = ok && (fldmod(7, 3) == (2, 1))
ok = ok && (fld1(1, 3) == 1)
ok = ok && (fld1(4, 3) == 2)
ok = ok && (fldmod1(6, 3) == (2, 3))
ok = ok && (mod1(4, 3) == 1)
ok
