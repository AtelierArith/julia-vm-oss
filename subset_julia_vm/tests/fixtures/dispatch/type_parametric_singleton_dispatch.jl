# Parametric type expressions dispatch as their Type{T} singleton.
# Mirrors Julia's selection of the Type{Complex{Float64}} method over
# the generic Type{T} fallback, while keeping ordinary Complex values separate
# from type objects (Issues #4039/#4044).

using Test

function dispatch_parametric_type_probe(::Type{Complex{Float64}}, d1)
    99
end

function dispatch_parametric_type_probe(::Type{Int64}, d1)
    33
end

function dispatch_parametric_type_probe(::Type{T}, d1) where T
    11
end

function dispatch_parametric_type_probe_from_var(T)
    dispatch_parametric_type_probe(T, 2)
end

function dispatch_type_or_value_probe(::Type{Complex{Float64}})
    1
end

function dispatch_type_or_value_probe(x::Complex{Float64})
    2
end

@testset "parametric Type singleton dispatch (Issues #4039/#4044)" begin
    @test dispatch_parametric_type_probe(Complex{Float64}, 2) == 99
    @test dispatch_parametric_type_probe(Int64, 2) == 33
    @test dispatch_parametric_type_probe(Float64, 2) == 11
    @test dispatch_parametric_type_probe_from_var(Complex{Float64}) == 99
    @test dispatch_type_or_value_probe(Complex{Float64}) == 1
    @test dispatch_type_or_value_probe(Complex{Float64}(3.0, 4.0)) == 2
end

true
