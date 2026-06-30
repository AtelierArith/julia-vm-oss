using Test

function check_scalar_flatten_collect(itr, expected_type, expected_values)
    result = collect(Base.Iterators.flatten(itr))
    ok = typeof(result) === Vector{expected_type}
    ok = ok && eltype(result) === expected_type
    ok = ok && length(result) == length(expected_values)
    for i in 1:length(expected_values)
        ok = ok && result[i] == expected_values[i]
        ok = ok && typeof(result[i]) === typeof(expected_values[i])
    end
    ok
end

@testset "scalar flatten collect (Issues #4018/#4664)" begin
    @test check_scalar_flatten_collect((1, 2), Int64, Any[1, 2])
    @test check_scalar_flatten_collect((1.0, 2.0), Float64, Any[1.0, 2.0])
    @test check_scalar_flatten_collect((1, 2.0), Real, Any[1, 2.0])
end

true
