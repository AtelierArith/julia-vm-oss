using Test

struct MethodPriorityCelsius
    value::Float64
end

function Base.convert(::Type{MethodPriorityCelsius}, x::Int64)
    return MethodPriorityCelsius(20.0)
end

function Base.convert(::Type{Float64}, s::String)
    return 9.0
end

@testset "convert method priority" begin
    c = convert(MethodPriorityCelsius, 20)
    @test c.value == 20.0

    @test convert(Float64, "1.5") == 9.0
    @test Base.convert(Float64, "2.5") == 9.0
end

true
