# Issue #10198: collect(eachmatch(re, s)) must materialize the RegexMatch
# elements instead of erroring "expected numeric value, got RegexMatch". The
# bug was in the array set-index (setindex!) path: storing a non-numeric,
# non-struct boxed value (RegexMatch/Regex/Function) into an Any array slot —
# which is exactly what collect's `result[i] = itr[i]` does — fell through
# every verbatim branch into the numeric f64 fallback. Comprehensions over the
# same iterator always worked, so this is specific to the collect/array
# materialization path. Values verified against upstream julia 1.12.
#
# NB: sjulia currently materializes eachmatch to a Vector{Any} (element-type
# loss, tracked under the #5073 umbrella), while upstream returns
# Vector{RegexMatch}. The element *values* match upstream exactly, so this
# fixture asserts length / element values / offsets rather than the container
# eltype.

using Test

@testset "collect(eachmatch(...)) materializes RegexMatch elements (Issue #10198)" begin
    ms = collect(eachmatch(r"a", "aba"))
    @test length(ms) == 2
    @test ms[1].match == "a"
    @test ms[2].match == "a"
    @test [m.match for m in ms] == ["a", "a"]
    @test ms[1].offset == 1
    @test ms[2].offset == 3

    # multi-character matches
    ns = collect(eachmatch(r"\d+", "a12b345"))
    @test length(ns) == 2
    @test [m.match for m in ns] == ["12", "345"]

    # no matches -> empty vector
    @test length(collect(eachmatch(r"x", "abc"))) == 0
    @test isempty(collect(eachmatch(r"x", "abc")))

    # collect agrees with the comprehension form that already worked
    @test length(collect(eachmatch(r"a", "aba"))) ==
          length([m for m in eachmatch(r"a", "aba")])
end

@testset "collect over an Any-vector of non-numeric boxed values (Issue #10198)" begin
    # setindex! of a RegexMatch into an Any array slot (collect's inner loop)
    rms = Any[match(r"a", "a")]
    got = collect(rms)
    @test length(got) == 1
    @test got[1].match == "a"

    # generalizes to other non-numeric boxed values (Regex, Function)
    @test length(collect(Any[r"x", r"y"])) == 2
    @test length(collect(Any[sin, cos])) == 2
end

true
