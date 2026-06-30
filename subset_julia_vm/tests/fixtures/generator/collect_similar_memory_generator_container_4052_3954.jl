function generator_memory_container_double_4052(x)
    return x * 2
end

function generator_memory_container_tofloat_4052(x)
    return x + 0.5
end

values = Base.collect_similar(
    Memory{Float64}(undef, 0),
    Base.Generator(generator_memory_container_double_4052, [1, 2, 3]),
)
@assert typeof(values) === Memory{Int64}
@assert eltype(values) === Int64
@assert length(values) == 3
@assert values[1] == 2
@assert values[2] == 4
@assert values[3] == 6

matrix_values = Base.collect_similar(
    Memory{Float64}(undef, 0),
    Base.Generator(generator_memory_container_double_4052, [1 2; 3 4]),
)
@assert typeof(matrix_values) === Matrix{Int64}
@assert size(matrix_values) == (2, 2)
@assert matrix_values == [2 4; 6 8]

empty = Base.collect_similar(
    Memory{Float64}(undef, 0),
    Base.Generator(generator_memory_container_tofloat_4052, Int64[]),
)
@assert typeof(empty) === Memory{Float64}
@assert eltype(empty) === Float64
@assert length(empty) == 0

function generator_memory_container_call_4052(cont, itr)
    return Base.collect_similar(cont, itr)
end

dynamic_values = generator_memory_container_call_4052(
    Memory{Float64}(undef, 0),
    Base.Generator(generator_memory_container_double_4052, [4, 5]),
)
@assert typeof(dynamic_values) === Memory{Int64}
@assert length(dynamic_values) == 2
@assert dynamic_values[1] == 8
@assert dynamic_values[2] == 10

true
