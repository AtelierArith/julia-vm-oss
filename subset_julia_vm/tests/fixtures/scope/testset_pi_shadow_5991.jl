using Test

@testset "testset local shadows Base.pi (Issue #5991)" begin
    pi = (series = [1, 2, 3],)
    @test pi.series[2] == 2
end

true
