using Test

macro tuple_where_type()
    esc(:(Tuple{T,S} where S))
end

function tuple_type_for(x::T) where T
    @tuple_where_type()
end

@testset "macro-returned where type binds introduced typevar" begin
    @test tuple_type_for(1) == (Tuple{Int64,S} where S)
    @test tuple_type_for(1.0) == (Tuple{Float64,S} where S)
end

true
