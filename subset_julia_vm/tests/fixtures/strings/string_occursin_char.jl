# Test occursin with Char needle (Issue #3570)
# Julia: occursin(c::AbstractChar, s::AbstractString) = any(==(c), s)

using Test

@testset "occursin(c::Char, s::String) - Issue #3570" begin
    # Present
    @test occursin('o', "foo") == true
    @test occursin('f', "foo") == true
    @test occursin(' ', "a b") == true
    @test occursin('\n', "a\nb") == true
    @test occursin('\t', "a\tb") == true

    # Absent
    @test occursin('z', "foo") == false
    @test occursin('a', "") == false
    @test occursin('A', "abc") == false
end

true
