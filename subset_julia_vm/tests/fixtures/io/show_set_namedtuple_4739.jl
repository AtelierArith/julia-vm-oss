using Test

@testset "show(io, ::AbstractSet) does not crash (Issue #4739)" begin
    s = Set([1])  # single-element so we can assert exact output
    @test repr(s) == "Set([1])"

    sf = Set(["a"])
    @test repr(sf) == "Set([\"a\"])"

    # NOTE: empty typed Set diverges between sjulia ("Set([])")
    # and upstream ("Set{Int64}()"). Type-parameter prefix gap
    # is tracked separately (same family as #4733).
end

@testset "repr(::NamedTuple) uses bare field names, not :name (Issue #4739)" begin
    @test repr((x=1, y=2)) == "(x = 1, y = 2)"
    @test repr((a="hi",)) == "(a = \"hi\",)"
    @test repr((value=42,)) == "(value = 42,)"
end

@testset "string(::NamedTuple) agrees with repr (Issue #4739)" begin
    nt = (x=1, y=2)
    @test string(nt) == "(x = 1, y = 2)"
end

true
