# findfirst: find first occurrence of value
# [1,2,3,2,4], findfirst(2.0) = 2

using Test

@testset "findfirst: find first occurrence (returns Int64 index)" begin
    a = [1.0, 2.0, 3.0, 2.0, 4.0]
    @test (findfirst(2.0, a)) == 2
end

true  # Test passed
