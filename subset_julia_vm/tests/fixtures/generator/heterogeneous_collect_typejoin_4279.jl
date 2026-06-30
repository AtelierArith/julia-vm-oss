using Test

@testset "heterogeneous generator collect typejoins numeric results (Issue #4279)" begin
    tuple_values = collect(x for x in (1, 2.0))
    @test typeof(tuple_values) === Vector{Real}
    @test eltype(tuple_values) === Real
    @test length(tuple_values) == 2
    @test tuple_values[1] == 1
    @test tuple_values[2] == 2.0

    branch_values = collect(i == 1 ? 1 : 2.0 for i in 1:2)
    @test typeof(branch_values) === Vector{Real}
    @test eltype(branch_values) === Real
    @test length(branch_values) == 2
    @test branch_values[1] == 1
    @test branch_values[2] == 2.0

    triple_values = collect(x for x in (1, 2.0, Int8(3)))
    @test typeof(triple_values) === Vector{Real}
    @test eltype(triple_values) === Real
    @test length(triple_values) == 3
    @test triple_values[1] == 1
    @test triple_values[2] == 2.0
    @test triple_values[3] == Int8(3)

    mixed_values = collect(x for x in (1, "a"))
    @test typeof(mixed_values) === Vector{Any}
    @test eltype(mixed_values) === Any
    @test length(mixed_values) == 2
    @test mixed_values[1] == 1
    @test mixed_values[2] == "a"

    triple_comprehension = [x for x in (1, 2.0, Int8(3))]
    @test typeof(triple_comprehension) === Vector{Real}
    @test eltype(triple_comprehension) === Real
    @test length(triple_comprehension) == 3
    @test triple_comprehension[1] == 1
    @test triple_comprehension[2] == 2.0
    @test triple_comprehension[3] == Int8(3)

    mixed_comprehension = [x for x in (1, "a")]
    @test typeof(mixed_comprehension) === Vector{Any}
    @test eltype(mixed_comprehension) === Any
    @test length(mixed_comprehension) == 2
    @test mixed_comprehension[1] == 1
    @test mixed_comprehension[2] == "a"
end

true
