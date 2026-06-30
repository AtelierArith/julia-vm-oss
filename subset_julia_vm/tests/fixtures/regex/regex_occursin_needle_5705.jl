using Test

# Issue #5705: occursin(re::Regex, s) must accept a Regex needle. The generic
# occursin(needle, haystack) calls ncodeunits(needle), which rejects a Regex, so a
# dedicated ::Regex-needle method (dispatching ahead of the generic) is required.

@testset "occursin with a Regex needle (Issue #5705)" begin
    @test occursin(r"\d+", "abc123") == true
    @test occursin(r"\d+", "abcdef") == false
    @test occursin(r"^a", "abc") == true
    @test occursin(r"^b", "abc") == false
    @test occursin(r"z", "abc") == false
    @test occursin(r"[A-Z]+", "helloWORLD") == true

    # Via a ::Regex-typed parameter (exercises Issue #5678 coercion too).
    f(re::Regex, s) = occursin(re, s)
    @test f(r"\d+", "abc123") == true

    # contains(haystack, needle) is the reverse-argument form of occursin.
    @test contains("abc123", r"\d+") == true
    @test contains("abcdef", r"\d+") == false

    # Plain string / Char needles are unaffected.
    @test occursin("bc", "abc") == true
    @test occursin("xy", "abc") == false
    @test occursin('b', "abc") == true
end

true
