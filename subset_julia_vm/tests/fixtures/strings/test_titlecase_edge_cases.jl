# titlecase edge cases - verify Pure Julia matches official Julia (Issue #2612)
# All assertions verified against: julia -e 'using Test; ...'

using Test

@testset "titlecase string edge cases" begin
    # Empty string
    @test titlecase("") == ""

    # Single character
    @test titlecase("a") == "A"
    @test titlecase("A") == "A"

    # Already titlecase
    @test titlecase("Hello World") == "Hello World"

    # All uppercase → titlecase (first letter upper, rest lower)
    @test titlecase("HELLO") == "Hello"
    @test titlecase("HELLO WORLD") == "Hello World"

    # All lowercase
    @test titlecase("hello world") == "Hello World"

    # Numbers and special characters (non-letter triggers next-letter capitalization)
    @test titlecase("hello123world") == "Hello123World"

    # Underscores and hyphens
    @test titlecase("hello_world") == "Hello_World"
    @test titlecase("hello-world") == "Hello-World"

    # Multiple spaces
    @test titlecase("hello  world") == "Hello  World"

    # Leading/trailing whitespace
    @test titlecase(" hello ") == " Hello "
end

true
