using Test

@testset "repr(missing) does not crash (Issue #4743)" begin
    @test repr(missing) == "missing"
    @test string(missing) == "missing"
    @test "$missing" == "missing"
end

@testset "show(io, ::Missing) writes 'missing' (Issue #4743)" begin
    io = IOBuffer()
    show(io, missing)
    @test String(take!(io)) == "missing"
end

@testset "Nothing show stays parity-correct alongside Missing" begin
    @test repr(nothing) == "nothing"
    @test string(nothing) == "nothing"
end

true
