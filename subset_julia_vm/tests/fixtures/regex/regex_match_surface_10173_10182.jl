# RegexMatch / Regex surface parity (Issues #10173, #10182)
#
# #10182: m.captures is Vector{Union{Nothing,SubString{String}}} and m.offsets
#         is Vector{Int} (not tuples), and show/repr prints the upstream
#         `RegexMatch("a", 1="a", 2=nothing)` form (named groups keyed by name).
# #10173: integer / Symbol / String capture indexing, keys, haskey, and Regex
#         field access `r.pattern`.
#
# Verified against upstream `julia` 1.12.

using Test

@testset "RegexMatch representation (#10182)" begin
    m = match(r"(a)(b)?", "a")
    # captures is a Vector, not a tuple
    @test m.captures isa Vector
    @test m.captures == ["a", nothing]
    @test length(m.captures) == 2
    @test m.captures[1] == "a"
    @test m.captures[2] === nothing
    # offsets is a Vector{Int}
    @test m.offsets isa Vector{Int}
    @test m.offsets == [1, 0]
    # upstream show / repr / string form
    @test repr(m) == "RegexMatch(\"a\", 1=\"a\", 2=nothing)"
    @test string(m) == "RegexMatch(\"a\", 1=\"a\", 2=nothing)"

    # named groups are keyed by name in the show form
    mn = match(r"(?<year>\d{4})-(?<month>\d{2})", "2026-07")
    @test repr(mn) == "RegexMatch(\"2026-07\", year=\"2026\", month=\"07\")"

    # no capture groups: empty typed vectors, bare show form
    m0 = match(r"\d+", "123")
    @test isempty(m0.captures)
    @test isempty(m0.offsets)
    @test repr(m0) == "RegexMatch(\"123\")"
end

@testset "RegexMatch indexing / keys / haskey (#10173)" begin
    m = match(r"(?<year>\d{4})-(?<month>\d{2})", "2026-07-10")
    # integer indexing == m.captures[i]
    @test m[1] == "2026"
    @test m[2] == "07"
    # Symbol and String named-capture access
    @test m[:year] == "2026"
    @test m["year"] == "2026"
    @test m[:month] == "07"
    @test m["month"] == "07"
    # every integer width is a valid key (upstream method is ::Integer)
    @test m[Int8(1)] == "2026"
    @test m[UInt16(2)] == "07"
    @test m[big(1)] == "2026"
    @test haskey(m, Int8(1))
    @test haskey(m, big(2))
    @test !haskey(m, Int8(5))
    # keys: named groups -> String names
    @test keys(m) == ["year", "month"]
    # haskey
    @test haskey(m, :year)
    @test haskey(m, "month")
    @test haskey(m, 1)
    @test !haskey(m, 3)
    @test !haskey(m, :nope)

    # unnamed groups: keys are 1-based integer indices
    mu = match(r"(a)(b)", "ab")
    @test keys(mu) == [1, 2]
    @test mu[1] == "a"
    @test mu[2] == "b"

    # mixed named + unnamed groups
    mm = match(r"(?<hour>\d+):(?<minute>\d+)(am|pm)?", "11:30")
    @test keys(mm) == ["hour", "minute", 3]
    @test mm[:hour] == "11"
    @test mm[3] === nothing

    # error paths mirror upstream
    @test_throws BoundsError m[0]
    @test_throws BoundsError m[5]
    @test_throws ErrorException m[:nope]
    @test_throws ErrorException m["nope"]
end

@testset "Regex field access (#10173)" begin
    r = r"ab+c"i
    @test r.pattern == "ab+c"
    r2 = r"(?<name>\w+)"
    @test r2.pattern == "(?<name>\\w+)"
end

true
