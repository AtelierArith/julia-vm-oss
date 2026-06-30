using Test

Base.:(==)(a::Vector{Int64}, b::Vector{Int64}) = false

@testset "array != uses generic !(==) dispatch (Issue #4276)" begin
    @test ([1, 2] == [1, 2]) == false
    @test ([1, 2] != [1, 2]) == true
end

true
