# Test @kwdef with required fields (fields without defaults)

using Test

@kwdef struct Person
    name::String  # Required - no default
    age::Int64 = 0
end

@testset "@kwdef with required fields" begin
    p1 = Person(name="Alice")
    @test p1.name == "Alice"
    @test p1.age == 0

    p2 = Person(name="Bob", age=30)
    @test p2.name == "Bob"
    @test p2.age == 30
end

true
