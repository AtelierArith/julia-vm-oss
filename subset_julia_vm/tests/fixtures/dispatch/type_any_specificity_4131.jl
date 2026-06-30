using Test

function dispatch_type_any_specificity_f(::Type)
    1
end

function dispatch_type_any_specificity_f(::Type{Any})
    2
end

function dispatch_type_any_specificity_g(::Type)
    1
end

function dispatch_type_any_specificity_g(::Type{T}) where {T}
    2
end

function dispatch_type_any_specificity_h(::Type{Any})
    1
end

function dispatch_type_any_specificity_h(::Type{Int64})
    2
end

function dispatch_type_any_specificity_h(::Type)
    3
end

@testset "Type{Any} method specificity (Issue #4131)" begin
    @test dispatch_type_any_specificity_f(Any) == 2
    @test dispatch_type_any_specificity_f(Int64) == 1

    @test dispatch_type_any_specificity_g(Any) == 2
    @test dispatch_type_any_specificity_g(Int64) == 2

    @test dispatch_type_any_specificity_h(Any) == 1
    @test dispatch_type_any_specificity_h(Int64) == 2
    @test dispatch_type_any_specificity_h(Float64) == 3
end

true
