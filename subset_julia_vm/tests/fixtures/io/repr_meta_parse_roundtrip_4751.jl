using Test

# Issue #4751: lock in the exact `repr(x)` output for the value
# categories sjulia and upstream Julia agree on. This is a regression
# guard for the family of repr/show fixes in Issues #4725 / #4727 /
# #4729 / #4731 / #4733 / #4735 / #4737 / #4739 / #4741 / #4743 /
# #4745 / #4747 / #4749 — each prior PR pinned one corner; this
# fixture pins them collectively so a regression in any common path
# fails loudly in the matrix.
#
# (Originally framed as `eval(Meta.parse(repr(x))) == x` round-trip,
# but sjulia's `Meta.parse`/`eval` has independent gaps —
# `nothing`/`missing`/`=>` etc. — that would mask the actual repr
# regressions we want to guard against. The literal-equality form
# below is stricter and platform-agnostic.)

@testset "repr output matrix: numeric primitives (Issue #4751)" begin
    @test repr(0) == "0"
    @test repr(42) == "42"
    @test repr(-7) == "-7"
    @test repr(typemax(Int64)) == "9223372036854775807"

    @test repr(1.5) == "1.5"
    @test repr(2.0) == "2.0"
    @test repr(0.0) == "0.0"
    @test repr(-0.0) == "-0.0"            # PR #4746 (#4745)

    @test repr(Float32(1.5)) == "1.5f0"   # PR #4748 (#4747)
    @test repr(Float32(-3.25)) == "-3.25f0"
    @test repr(Float16(1.5)) == "Float16(1.5)"
end

@testset "repr output matrix: strings and chars (Issue #4751)" begin
    @test repr("") == "\"\""
    @test repr("hello") == "\"hello\""
    @test repr("a\nb") == "\"a\\nb\""        # PR #4750 (#4749)
    @test repr("with\"quote") == "\"with\\\"quote\""
    @test repr("with\\backslash") == "\"with\\\\backslash\""

    @test repr('a') == "'a'"
    @test repr('\n') == "'\\n'"            # PR #4750
    @test repr('\\') == "'\\\\'"
end

@testset "repr output matrix: symbols, Bool, Nothing, Missing (Issue #4751)" begin
    @test repr(:foo) == ":foo"
    @test repr(:x) == ":x"
    @test repr(true) == "true"
    @test repr(false) == "false"
    @test repr(nothing) == "nothing"
    @test repr(missing) == "missing"       # PR #4744 (#4743)
end

@testset "repr output matrix: Pair and small containers (Issue #4751)" begin
    @test repr(Pair(1, 2)) == "1 => 2"     # PR #4726 (#4725)
    @test repr(Pair("a", 42)) == "\"a\" => 42"

    @test repr([1, 2, 3]) == "[1, 2, 3]"   # PR #4732 (#4731)
    @test repr([1.0, 2.0]) == "[1.0, 2.0]"
    @test repr(Int[]) == "Int64[]"          # PR #4734 (#4733)

    @test repr((1, 2, 3)) == "(1, 2, 3)"
    @test repr((1, "two")) == "(1, \"two\")"
end

true
