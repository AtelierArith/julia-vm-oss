# Test @kwdef with required fields (fields without defaults)

using Test

@kwdef struct Person
    name::String
    age::Int64 = 0
end

@testset "@kwdef with required fields" begin
    # Required field with default for optional
    p1 = Person(name="Alice")
    @test p1.name == "Alice"
    @test p1.age == 0

    # Both fields provided
    p2 = Person(name="Bob", age=25)
    @test p2.name == "Bob"
    @test p2.age == 25

    # Test kwargs in different order
    p3 = Person(age=30, name="Charlie")
    @test p3.name == "Charlie"
    @test p3.age == 30
end

true
