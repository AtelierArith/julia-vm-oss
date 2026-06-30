using Test

@testset "Matrix arraymath addition and subtraction (Issue #7579)" begin
    A = [1.0 0.0; 0.0 2.0]
    @test A + A == [2.0 0.0; 0.0 4.0]
    S = A + A
    @test size(S, 1) == 2
    @test size(S, 2) == 2

    B = [1 2; 3 4]
    C = [0.5 1.5; 2.5 3.5]
    @test B + C == [1.5 3.5; 5.5 7.5]
    @test B - B == [0 0; 0 0]

    threw = false
    try
        [1 2] + [1; 2]
    catch
        threw = true
    end
    @test threw
end

true
