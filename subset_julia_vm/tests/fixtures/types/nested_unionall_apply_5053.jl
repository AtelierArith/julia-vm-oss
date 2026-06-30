using Test

@testset "multi-var UnionAll application (Issue #5053)" begin
    array_schema = Array{T,N} where {T,N}
    @test array_schema{Int,2} === Array{Int,2}
    @test array_schema{Float64,1} === Vector{Float64}
    @test Core.apply_type(array_schema, Int, 2) === Array{Int,2}

    tuple_schema = Tuple{T,U} where {T,U}
    @test tuple_schema{Int,String} === Tuple{Int,String}
    @test Core.apply_type(tuple_schema, Int, String) === Tuple{Int,String}

    nested_schema = Vector{Tuple{T,U}} where {T,U}
    @test nested_schema{Int,String} === Vector{Tuple{Int,String}}
    @test Core.apply_type(nested_schema, Int, String) === Vector{Tuple{Int,String}}

    # Uppercase aliases are ordinary UnionAll-valued bindings here, not static
    # type-alias declarations; applying them must still instantiate the nested
    # body variables.
    T2 = Vector{Tuple{T,U}} where {T,U}
    @test T2{Int,String} === Vector{Tuple{Int,String}}
    @test Core.apply_type(T2, Int, String) === Vector{Tuple{Int,String}}
end

true
