# Abstract type hierarchy with struct inheritance

using Test

abstract type Animal end

abstract type Mammal <: Animal end

abstract type Bird <: Animal end

struct Dog <: Mammal
    name::String
end

struct Eagle <: Bird
    wingspan::Float64
end

@testset "Abstract type hierarchy with struct inheritance" begin



    d = Dog("Rex")
    e = Eagle(2.0)

    result = 0.0
    result += isa(d, Dog) ? 1.0 : 0.0       # true
    result += isa(d, Mammal) ? 1.0 : 0.0    # true
    result += isa(d, Animal) ? 1.0 : 0.0    # true
    result += isa(d, Bird) ? 1.0 : 0.0      # false
    result += isa(e, Bird) ? 1.0 : 0.0      # true
    result += isa(e, Animal) ? 1.0 : 0.0    # true
    result += isa(e, Mammal) ? 1.0 : 0.0    # false
    @test (result) == 5.0
end

true  # Test passed
