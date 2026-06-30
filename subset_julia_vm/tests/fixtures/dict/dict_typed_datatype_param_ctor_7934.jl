using Test

# Issue #7934: typed Dict constructors whose type parameters are DataType values
# (`Dict{Type, Dict{Symbol, Any}}()`) construct and index correctly.
@testset "Issue #7934: typed Dict constructors with DataType params" begin
    storage = Dict{Type, Dict{Symbol, Any}}()
    storage[Int] = Dict{Symbol, Any}()
    storage[Int][:a] = 10
    @test storage[Int][:a] == 10
    @test storage isa Dict{Type, Dict{Symbol, Any}}

    inner = Dict{Symbol, Any}()
    @test inner isa Dict{Symbol, Any}
end

true
