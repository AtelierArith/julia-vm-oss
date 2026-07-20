# Bitwise Bool/Missing three-valued (Kleene) logic (Issue #10692)
#
# Upstream base/missing.jl: `false & missing` is `false`, `true | missing`
# is `true`, and every other bitwise combination involving missing is
# missing — including the Integer arms.

using Test

@testset "three-valued & " begin
    @test ismissing(missing & missing)
    @test ismissing(true & missing)
    @test ismissing(missing & true)
    @test (false & missing) === false
    @test (missing & false) === false
    @test ismissing(missing & 1)
    @test ismissing(2 & missing)
end

@testset "three-valued |" begin
    @test ismissing(missing | missing)
    @test (true | missing) === true
    @test (missing | true) === true
    @test ismissing(false | missing)
    @test ismissing(missing | false)
    @test ismissing(missing | 1)
    @test ismissing(2 | missing)
end

@testset "three-valued xor" begin
    @test ismissing(xor(missing, missing))
    @test ismissing(xor(missing, true))
    @test ismissing(xor(false, missing))
    @test ismissing(missing ⊻ true)
    @test ismissing(xor(missing, 3))
    @test ismissing(xor(3, missing))
end

true
