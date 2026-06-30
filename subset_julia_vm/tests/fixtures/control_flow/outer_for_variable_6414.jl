using Test

@testset "outer as for-loop variable (Issue #6414)" begin
    total = 0
    for outer in 1:2
        total += outer
    end
    @test total == 3
end

true
