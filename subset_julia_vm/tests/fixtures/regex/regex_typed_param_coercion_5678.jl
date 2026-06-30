using Test

@testset "Regex / RegexMatch typed parameter coercion (Issue #5678)" begin
    # ::Regex parameter used with match() — the original repro.
    k(s, re::Regex) = match(re, s).offset
    @test k("hello", r"l") == 3

    # ::Regex parameter returned (widens to Any), identity preserved.
    pick(re::Regex) = re
    @test pick(r"x") isa Regex

    # ::Regex parameter passed through to another function (boxed as Any).
    use(re::Regex, s) = match(re, s).match
    relay(re::Regex, s) = use(re, s)
    @test relay(r"\d+", "abc123") == "123"

    # ::RegexMatch parameter (widens to Any).
    firstcap(m::RegexMatch) = m.captures[1]
    @test firstcap(match(r"(\d)(\d)", "ab12cd")) == "1"

    # ::Regex parameter stored in a local then used.
    function via_local(re::Regex, s)
        r = re
        return match(r, s).offset
    end
    @test via_local(r"\d+", "abc123") == 4
end

true
