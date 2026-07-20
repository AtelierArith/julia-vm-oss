using Test

f10138(x) = 1
p10138(x) = false

@testset "all-filtered generator with user-call predicate uses empty semantics (Issue #10138)" begin
    from_body_and_predicate_call = collect(f10138(x) for x in [1, 2, 3] if f10138(x) >= 2)
    @test isempty(from_body_and_predicate_call)
    @test typeof(from_body_and_predicate_call) == Vector{Union{}}
    @test eltype(from_body_and_predicate_call) == Union{}
    @test_throws ArgumentError sum(f10138(x) for x in [1, 2, 3] if f10138(x) >= 2)

    from_operator_body_and_predicate_call = collect(x * x for x in [1, 2, 3] if p10138(x))
    @test isempty(from_operator_body_and_predicate_call)
    @test typeof(from_operator_body_and_predicate_call) == Vector{Union{}}
    @test_throws ArgumentError sum(x * x for x in [1, 2, 3] if p10138(x))
    @test_throws ArgumentError first(f10138(x) for x in [1, 2, 3] if p10138(x))
end

@testset "transparent filtered generators keep inferred empty eltype" begin
    literal_predicate = collect(f10138(x) for x in [1, 2, 3] if x >= 4)
    @test isempty(literal_predicate)
    @test typeof(literal_predicate) == Vector{Int64}

    base_predicate = collect(x for x in [2, 4] if isodd(x))
    @test isempty(base_predicate)
    @test typeof(base_predicate) == Vector{Int64}
    @test sum(x for x in [2, 4] if isodd(x)) == 0
end

true
