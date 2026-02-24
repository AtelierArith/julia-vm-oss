# Test exported string functions

using Test

@testset "String manipulation functions" begin
    # chomp - remove trailing newline
    @test chomp("hello\n") === "hello"
    @test chomp("hello") === "hello"

    # chop - remove last character
    @test chop("hello") === "hell"

    # contains - check if substring exists
    @test contains("hello world", "world")
    @test !contains("hello", "xyz")

    # startswith/endswith
    @test startswith("hello", "he")
    @test endswith("hello", "lo")
    @test !startswith("hello", "lo")
    @test !endswith("hello", "he")

    # strip/lstrip/rstrip
    @test strip("  hello  ") === "hello"
    @test lstrip("  hello") === "hello"
    @test rstrip("hello  ") === "hello"

    # join
    @test join(["a", "b", "c"], ", ") === "a, b, c"
    @test join(["x"], "-") === "x"

    # occursin
    @test occursin("ell", "hello")
    @test !occursin("xyz", "hello")

    # uppercasefirst/lowercasefirst
    @test uppercasefirst("hello") === "Hello"
    @test lowercasefirst("Hello") === "hello"

    # replace with Pair
    @test replace("hello world", "world" => "Julia") === "hello Julia"

    # escape_string
    @test escape_string("a\nb") === "a\\nb"
end

true
