# RegexMatch field access: m.match, m.captures, m.offset, m.offsets
# Issue #2116

using Test

@testset "RegexMatch field access" begin
    # Basic field access
    m = match(r"(\d+)", "abc123def")
    @test m.match == "123"
    @test m.offset == 4
    @test m.captures == ("123",)
    @test m.offsets == (4,)

    # Multiple capture groups
    m2 = match(r"(\w+)@(\w+)", "user@host")
    @test m2.match == "user@host"
    @test m2.captures == ("user", "host")
    @test m2.offset == 1

    # No capture groups
    m3 = match(r"\d+", "abc123def")
    @test m3.match == "123"
    @test m3.offset == 4
    @test m3.captures == ()

    # Match at beginning
    m4 = match(r"(\w+)", "hello world")
    @test m4.match == "hello"
    @test m4.offset == 1

    # nothing check still works
    m5 = match(r"xyz", "hello")
    @test m5 === nothing
end

true
