using Test

greet(name, greeting="Hello") = string(greeting, " ", name)
power(x, n=2) = x^n

@testset "function default arguments" begin
    @test greet("World") == "Hello World"
    @test greet("World", "Hi") == "Hi World"
    @test power(3) == 9
    @test power(3, 3) == 27
    @test power(2, 10) == 1024
end

true
