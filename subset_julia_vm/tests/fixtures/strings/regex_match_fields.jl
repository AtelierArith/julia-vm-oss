# Test RegexMatch field access (Issue #2116)

using Test

@testset "RegexMatch field access" begin
    m = match(r"(\d+)", "abc123def")
    @test m !== nothing

    # .match - the matched substring
    @test m.match == "123"

    # .offset - starting position (1-based)
    @test m.offset == 4

    # .captures - tuple of captured groups
    caps = m.captures
    @test caps[1] == "123"

    # .offsets - starting positions of each capture group
    offs = m.offsets
    @test offs[1] == 4
end

@testset "RegexMatch multiple captures" begin
    m = match(r"(\w+)@(\w+)\.(\w+)", "user@example.com")
    @test m !== nothing
    @test m.match == "user@example.com"
    @test m.captures[1] == "user"
    @test m.captures[2] == "example"
    @test m.captures[3] == "com"
    @test m.offset == 1
end

@testset "RegexMatch no match returns nothing" begin
    m = match(r"xyz", "abc")
    @test m === nothing
end

true
