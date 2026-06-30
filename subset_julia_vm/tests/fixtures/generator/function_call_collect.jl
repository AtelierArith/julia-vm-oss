# Generator expression whose body is a direct function call.
# This guards the VM lazy Generator path from returning untransformed inner
# iterator values (Issues #3961, #3966).

using Test

double(x) = x * 2
to_float(x) = x + 0.5

@testset "Generator function-call collect" begin
    ints = collect(double(x) for x in [1, 2, 3])
    @test typeof(ints) === Vector{Int64}
    @test ints == [2, 4, 6]
    @test eltype(ints) === Int64

    floats = collect(to_float(x) for x in [1, 2, 3])
    @test typeof(floats) === Vector{Float64}
    @test floats == [1.5, 2.5, 3.5]
    @test eltype(floats) === Float64

    g = (double(x) for x in [1, 2, 3])
    assigned = collect(g)
    @test typeof(assigned) === Vector{Int64}
    @test assigned == [2, 4, 6]
    @test eltype(assigned) === Int64

    lazy = collect(Base.Generator(double, [1, 2, 3]))
    @test typeof(lazy) === Vector{Int64}
    @test lazy == [2, 4, 6]
    @test eltype(lazy) === Int64

    empty = collect(double(x) for x in Int64[])
    @test typeof(empty) === Vector{Int64}
    @test eltype(empty) === Int64
    @test length(empty) == 0
end

true
