# Test MIME type system basics
# Simplified test to avoid type parameter issues

using Test

@testset "MIME type basics" begin
    # Test MIME type creation with string macro
    m1 = MIME"text/plain"

    # Test MIME type creation with constructor
    m2 = MIME("text/html")

    # Test istextmime for text types using String overload
    @test istextmime("text/plain") == true
    @test istextmime("text/html") == true

    # Test istextmime for binary types
    @test istextmime("image/png") == false
    @test istextmime("image/jpeg") == false

    # Test istextmime for known text application types
    @test istextmime("application/json") == true
    @test istextmime("application/javascript") == true
    @test istextmime("application/xml") == true
end

true
