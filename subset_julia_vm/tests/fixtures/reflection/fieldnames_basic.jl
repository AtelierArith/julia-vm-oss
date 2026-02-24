# Test fieldnames

using Test

struct Person
    name::String
    age::Int64
end

@testset "fieldnames - tuple of field names (length returns Int64)" begin

    # fieldnames(Person) should return (:name, :age)
    names = fieldnames(Person)
    @test (length(names)) == 2
end

true  # Test passed
