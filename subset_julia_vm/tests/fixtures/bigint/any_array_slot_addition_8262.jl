using Test

@testset "BigInt addition through Any array slot" begin
    a = Any[big(0)]
    a[1] = a[1] + big(1)

    @test a[1] == big(1)
    @test typeof(a[1]) === BigInt
end

true
