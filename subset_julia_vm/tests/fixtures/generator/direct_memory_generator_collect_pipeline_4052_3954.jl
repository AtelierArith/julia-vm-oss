function generator_direct_memory_double_4052(x)
    return x * 2
end

function generator_direct_memory_tofloat_4052(x)
    return x + 0.5
end

g = Base.Generator(generator_direct_memory_tofloat_4052, [1, 2])
values = Base._collect(Memory{Int64}(undef, 0), g, Base.IteratorEltype(g), Base.IteratorSize(g))
@assert typeof(values) === Memory{Float64}
@assert eltype(values) === Float64
@assert length(values) == 2
@assert values[1] == 1.5
@assert values[2] == 2.5

empty_g = Base.Generator(generator_direct_memory_tofloat_4052, Int64[])
empty_values = Base._collect(Memory{Int64}(undef, 0), empty_g, Base.IteratorEltype(empty_g), Base.IteratorSize(empty_g))
@assert typeof(empty_values) === Memory{Float64}
@assert eltype(empty_values) === Float64
@assert length(empty_values) == 0

matrix_g = Base.Generator(generator_direct_memory_double_4052, [1 2; 3 4])
matrix_values = Base._collect(Memory{Int64}(undef, 0), matrix_g, Base.IteratorEltype(matrix_g), Base.IteratorSize(matrix_g))
@assert typeof(matrix_values) === Matrix{Int64}
@assert eltype(matrix_values) === Int64
@assert size(matrix_values) == (2, 2)
@assert matrix_values == [2 4; 6 8]

true
