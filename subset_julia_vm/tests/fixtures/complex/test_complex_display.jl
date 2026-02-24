using Test

@testset "Complex display formatting" begin
    # Int64 complex
    @test string(1 + 2im) == "1 + 2im"
    @test string(0 + 0im) == "0 + 0im"

    # Float64 complex - the key fix: must show ".0"
    @test string(3.0 + 2.0im) == "3.0 + 2.0im"
    @test string(0.0 + 0.0im) == "0.0 + 0.0im"

    # Negative imaginary
    @test string(3.0 - 4.0im) == "3.0 - 4.0im"
    @test string(1 - 2im) == "1 - 2im"
end

true
