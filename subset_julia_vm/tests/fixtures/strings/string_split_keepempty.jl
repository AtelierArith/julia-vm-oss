using Test

# Regression test for Issue #3651:
# split(s, delim; keepempty=...) and rsplit(s, delim; keepempty=...) must
# both honor the keyword. Previously the keyword was silently dropped and
# both forms returned all parts including empties.

@testset "split keepempty (#3651)" begin
    # default keepempty=true: keep "" between consecutive delims and at ends
    @test split(",a,,b,", ",") == ["", "a", "", "b", ""]
    @test split(",a,,b,", ","; keepempty=true) == ["", "a", "", "b", ""]
    # keepempty=false: drop empties
    @test split(",a,,b,", ","; keepempty=false) == ["a", "b"]
    # combined with limit (limit applies first; then keepempty=false filters)
    @test split(",a,,b,c", ","; limit=3, keepempty=false) == ["a", "b", "c"]
    # Char delimiter
    @test split(",a,,b,", ','; keepempty=false) == ["a", "b"]
    # No empties to drop — keepempty=false is no-op
    @test split("a,b,c", ","; keepempty=false) == ["a", "b", "c"]
end

@testset "rsplit keepempty (#3651)" begin
    # default keepempty=true
    @test rsplit(",a,,b,", ",") == ["", "a", "", "b", ""]
    @test rsplit(",a,,b,", ","; keepempty=true) == ["", "a", "", "b", ""]
    # keepempty=false
    @test rsplit(",a,,b,", ","; keepempty=false) == ["a", "b"]
    # combined with limit (rsplit limit splits from the right; then filter empties)
    @test rsplit(",a,,b,", ","; limit=3, keepempty=false) == ["a", "b"]
    # Char delimiter
    @test rsplit(",a,,b,", ','; keepempty=false) == ["a", "b"]
end

true
