using Test

function assigned_arrow_where_body_11031(x::Float64) where Float64
    f = () -> Vector{Float64}
    f()
end

function nested_function_where_body_11031(x::Float64) where Float64
    function inner_11031()
        Vector{Float64}
    end
    inner_11031()
end

function nested_short_function_where_body_11031(x::Float64) where Float64
    inner_short_11031() = Vector{Float64}
    inner_short_11031()
end

@testset "where binders remain lexical in closure bodies (Issue #11031)" begin
    @test assigned_arrow_where_body_11031(1) === Vector{Int64}
    @test assigned_arrow_where_body_11031(1.0) === Vector{Float64}

    @test nested_function_where_body_11031(1) === Vector{Int64}
    @test nested_function_where_body_11031(1.0) === Vector{Float64}

    @test nested_short_function_where_body_11031(1) === Vector{Int64}
    @test nested_short_function_where_body_11031(1.0) === Vector{Float64}
end

true
