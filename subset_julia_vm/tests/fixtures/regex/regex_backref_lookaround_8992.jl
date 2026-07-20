using Test

@testset "Regex backreferences and lookaround (Issue #8992)" begin
    @test match(r"(a)\1", "aa") !== nothing
    @test match(r"(a)\1", "ab") === nothing

    lookahead = match(r"a(?=b)", "ab")
    @test lookahead !== nothing
    @test lookahead.match == "a"
    @test lookahead.offset == 1

    lookbehind = match(r"(?<=a)b", "ab")
    @test lookbehind !== nothing
    @test lookbehind.match == "b"
    @test lookbehind.offset == 2

    @test match(r"a(?!c)", "ab") !== nothing
    @test match(r"a(?!c)", "ac") === nothing
    @test match(r"(?<!c)b", "ab") !== nothing
    @test match(r"(?<!c)b", "cb") === nothing
end

true
