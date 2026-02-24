# Test r"..." regex literal syntax

using Test

@testset "Regex literal syntax" begin
    # Basic regex literal creates a Regex value that can be used in occursin
    r = r"hello"
    @test occursin(r, "hello world") == true

    # Regex literal with special characters
    r2 = r"a.*b"
    @test occursin(r2, "aXXXb") == true

    # Regex literal with digits
    r3 = r"\d+"
    @test occursin(r3, "abc123def") == true
    @test occursin(r3, "no digits") == false
end

true
