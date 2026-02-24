# Test chop function - remove characters from start/end of string

using Test

@testset "chop - default (remove last character)" begin
    @test chop("hello") == "hell"
    @test chop("a") == ""
    @test chop("") == ""
    @test chop("hello world") == "hello worl"
    @test chop("hello!") == "hello"
    @test chop("hello\n") == "hello"
end

@testset "chop with head and tail keywords (Issue #2045)" begin
    # head removes from start, tail removes from end
    @test chop("hello", head=2, tail=0) == "llo"
    @test chop("hello", head=0, tail=2) == "hel"
    @test chop("hello", head=1, tail=1) == "ell"

    # head=0, tail=0: no removal
    @test chop("hello", head=0, tail=0) == "hello"

    # Remove everything
    @test chop("ab", head=1, tail=1) == ""

    # Excess removal: returns empty
    @test chop("ab", head=5, tail=5) == ""

    # Only head
    @test chop("hello", head=3, tail=0) == "lo"

    # Only tail (different from default)
    @test chop("hello", head=0, tail=3) == "he"
end

true
