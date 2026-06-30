using Test

collect_runtime_any_tuple(x) = collect(x)

@testset "collect tuple type preservation" begin
    @test typeof(Base.IteratorEltype((1, 2.0))) === typeof(Base.HasEltype())
    @test eltype(()) === Union{}
    @test eltype((1, 2.0)) === Real
    @test eltype((1, true)) === Integer
    @test eltype((Int32(1), Int64(2))) === Signed
    @test eltype((1, "a")) === Any

    ints = collect((1, 2, 3))
    @test typeof(ints) === Vector{Int64}
    @test ints == [1, 2, 3]

    floats = collect((1.0, 2.0))
    @test typeof(floats) === Vector{Float64}
    @test floats == [1.0, 2.0]

    int32s = collect((Int32(1), Int32(2)))
    @test typeof(int32s) === Vector{Int32}
    @test length(int32s) == 2
    @test int32s[1] == Int32(1)
    @test int32s[2] == Int32(2)

    empty = collect(())
    @test typeof(empty) === Vector{Union{}}
    @test length(empty) == 0

    runtime_empty = collect_runtime_any_tuple(())
    @test typeof(runtime_empty) === Vector{Union{}}
    @test length(runtime_empty) == 0

    explicit_bottom = Vector{Union{}}(undef, 0)
    @test typeof(explicit_bottom) === Vector{Union{}}
    @test length(explicit_bottom) == 0
end

@testset "collect heterogeneous tuple type widening" begin
    real_values = collect((1, 2.0))
    @test typeof(real_values) === Vector{Real}
    @test real_values[1] == 1
    @test real_values[2] == 2.0

    integer_values = collect((1, true))
    @test typeof(integer_values) === Vector{Integer}
    @test integer_values[1] == 1
    @test integer_values[2] == true

    signed_values = collect((Int32(1), Int64(2)))
    @test typeof(signed_values) === Vector{Signed}
    @test signed_values[1] == Int32(1)
    @test signed_values[2] == Int64(2)

    any_values = collect((1, "a"))
    @test typeof(any_values) === Vector{Any}
    @test any_values[1] == 1
    @test any_values[2] == "a"

    cases = ((1, 2.0), (1, true), (Int32(1), Int64(2)), (1, "a"))
    from_index = collect(cases[1])
    @test typeof(from_index) === Vector{Real}

    runtime_any = collect_runtime_any_tuple(cases[1])
    @test typeof(runtime_any) === Vector{Real}

    runtime_integer = collect_runtime_any_tuple(cases[2])
    @test typeof(runtime_integer) === Vector{Integer}
    @test runtime_integer[1] == 1
    @test runtime_integer[2] == true

    runtime_signed = collect_runtime_any_tuple(cases[3])
    @test typeof(runtime_signed) === Vector{Signed}
    @test runtime_signed[1] == Int32(1)
    @test runtime_signed[2] == Int64(2)

    runtime_any_values = collect_runtime_any_tuple(cases[4])
    @test typeof(runtime_any_values) === Vector{Any}
    @test runtime_any_values[1] == 1
    @test runtime_any_values[2] == "a"

    widened = Any[]
    for c in cases
        push!(widened, eltype(collect(c)))
    end
    @test widened[1] === Real
    @test widened[2] === Integer
    @test widened[3] === Signed
    @test widened[4] === Any
end

true
