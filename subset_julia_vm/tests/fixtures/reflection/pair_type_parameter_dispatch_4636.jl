using Test

function pairdispatch4636(::Type{Pair}, dims::Tuple)
    return :bare
end

function pairdispatch4636(::Type{Pair{K,V}}, dims::Tuple) where {K,V}
    return :param
end

@testset "Type{Pair{K,V}} beats Type{Pair} dispatch (#4636)" begin
    @test pairdispatch4636(Pair{Int64, Int8}, (2,)) == :param

    m1 = which(pairdispatch4636, Tuple{Type{Pair{Int64, Int8}}, Tuple{Int64}})
    @test m1.name == :pairdispatch4636
    @test occursin("Pair{K", string(m1.sig))

    m2 = which(pairdispatch4636, (Type{Pair{Int64, Int8}}, Tuple{Int64}))
    @test m2.name == :pairdispatch4636
    @test occursin("Pair{K", string(m2.sig))
end

true
