using Test

@testset "Mixed numeric + Complex array promotion" begin
    # [1, 2, im] should produce Vector{Complex{Int64}}
    a = [1, 2, im]
    @test length(a) == 3
    @test a[1] == Complex{Int64}(1, 0)
    @test a[2] == Complex{Int64}(2, 0)
    @test a[3] == Complex{Int64}(0, 1)

    # Type parameter check via dispatch
    f(x::Vector{T}) where {T} = T
    @test f([1, 2, im]) == Complex{Int64}
end

true
