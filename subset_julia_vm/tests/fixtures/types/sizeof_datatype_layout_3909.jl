using Test

struct LayoutBits3909
    x::Int64
    y::Bool
end

struct LayoutRefs3909
    name::String
    x::Int64
end

mutable struct MutableLayout3909
    x::Int64
end

struct EmptyLayout3909
end

@testset "sizeof(::DataType) uses runtime layout metadata (Issue #3909)" begin
    @test sizeof(Bool) == 1
    @test sizeof(Char) == 4
    @test sizeof(Int8) == 1
    @test sizeof(Nothing) == 0
    @test sizeof(Missing) == 0

    @test sizeof(LayoutBits3909) == 16
    @test sizeof(LayoutRefs3909) == 16
    @test sizeof(MutableLayout3909) == 8
    @test sizeof(EmptyLayout3909) == 0

    @test isbitstype(LayoutBits3909)
    @test !isbitstype(LayoutRefs3909)
    @test !isbitstype(MutableLayout3909)
end

true
