using Test

# Regression test for Issue #3668:
# `strip`, `lstrip`, `rstrip` must accept a 2-arg `(s::String, c::Char)`
# form that strips occurrences of `c` from the appropriate end(s).
# Previously only the 1-arg whitespace form and predicate form existed.

@testset "strip(s, ::Char) (#3668)" begin
    @test strip("xxhelloxx", 'x') == "hello"
    @test strip("aabbccaa", 'a') == "bbcc"
    @test strip("xxhellox", 'x') == "hello"
    @test strip("xxx", 'x') == ""
    @test strip("", 'x') == ""
    @test strip("hello", 'x') == "hello"   # no match
    @test strip("hello", 'h') == "ello"    # only one end
end

@testset "lstrip(s, ::Char) (#3668)" begin
    @test lstrip("xxhello", 'x') == "hello"
    @test lstrip("xxhelloxx", 'x') == "helloxx"   # only left
    @test lstrip("hello", 'x') == "hello"
    @test lstrip("xxx", 'x') == ""
    @test lstrip("", 'x') == ""
end

@testset "rstrip(s, ::Char) (#3668)" begin
    @test rstrip("helloxx", 'x') == "hello"
    @test rstrip("xxhelloxx", 'x') == "xxhello"   # only right
    @test rstrip("hello", 'x') == "hello"
    @test rstrip("xxx", 'x') == ""
    @test rstrip("", 'x') == ""
end

@testset "1-arg whitespace strip (regression)" begin
    @test strip("  hello  ") == "hello"
    @test lstrip("  hello") == "hello"
    @test rstrip("hello  ") == "hello"
end

@testset "predicate-form strip (regression)" begin
    @test strip(c -> c == 'x', "xxhelloxx") == "hello"
    @test lstrip(c -> c == 'x', "xxhello") == "hello"
    @test rstrip(c -> c == 'x', "helloxx") == "hello"
end

true
