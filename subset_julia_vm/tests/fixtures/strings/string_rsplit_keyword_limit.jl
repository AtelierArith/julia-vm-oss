using Test

# Regression test for Issue #3610:
# `rsplit(s, delim; limit=N)` keyword form must thread `limit` through to the
# positional impl. Previously the keyword was silently dropped and the full
# split was returned.

@testset "rsplit keyword limit (#3610)" begin
    # Basic case from the Issue MWE
    @test rsplit("a,b,c", ","; limit=2) == ["a,b", "c"]

    # Char delimiter
    @test rsplit("a,b,c", ','; limit=2) == ["a,b", "c"]

    # Larger limit keeps more rightmost splits
    @test rsplit("a,b,c,d", ","; limit=3) == ["a,b", "c", "d"]
    @test rsplit("a,b,c,d", ","; limit=2) == ["a,b,c", "d"]

    # limit=1 keeps the whole string as one part
    @test rsplit("a,b,c", ","; limit=1) == ["a,b,c"]

    # limit=0 means no limit (Julia default)
    @test rsplit("a,b,c", ","; limit=0) == ["a", "b", "c"]

    # Default keyword (no limit specified) matches limit=0
    @test rsplit("a,b,c", ",") == ["a", "b", "c"]
end

true
