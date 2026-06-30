using Test
using Base.Iterators

import Base: Float64

struct GeneratorDataTypeInput3910
    x::Int64
end

Float64(x::GeneratorDataTypeInput3910) = 3910.0

function collect_map_type_3910(T, xs)
    return collect(Iterators.map(T, xs))
end

@testset "Generator DataType callable dispatch (Issue #3910)" begin
    xs = [GeneratorDataTypeInput3910(1)]

    direct_values = collect(Iterators.map(Float64, xs))
    @test direct_values == [3910.0]

    runtime_values = collect_map_type_3910(Float64, xs)
    @test runtime_values == [3910.0]
end

true
