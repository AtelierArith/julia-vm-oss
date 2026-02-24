# replace(s, r"pattern" => new) with Regex patterns
# Issue #2112

using Test

@testset "replace with regex pattern" begin
    # Basic regex replace
    @test replace("hello world", r"world" => "julia") == "hello julia"

    # Replace all occurrences (default count=0)
    @test replace("aaa", r"a" => "b") == "bbb"

    # Replace with count limit
    @test replace("aaa", r"a" => "b", count=1) == "baa"
    @test replace("aaa", r"a" => "b", count=2) == "bba"

    # Pattern not found
    @test replace("hello", r"xyz" => "abc") == "hello"

    # Replace with empty string (deletion)
    @test replace("hello world", r" world" => "") == "hello"

    # 2-arg string replace still works
    @test replace("hello world", "world" => "julia") == "hello julia"
end

true
