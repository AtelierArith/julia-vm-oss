using Test

# Issue #10176: split(s, ::Regex) dispatch — a `split(::String, ::Regex; limit,
# keepempty)` method now reaches the regex splitter, mirroring upstream Julia's
# SplitIterator semantics (limit / keepempty, SubString{String} results).
# rsplit(::Regex) is intentionally NOT provided — upstream julia 1.12 itself
# MethodErrors on it (findprev(::Regex, ...) is undefined).
# Verified against upstream julia 1.12.

@testset "split by Regex — basic (Issue #10176)" begin
    @test split("a1b22c333d", r"\d+") == ["a", "b", "c", "d"]
    @test split("a, b,  c", r",\s*") == ["a", "b", "c"]
    @test split("hello", r"z") == ["hello"]
    @test split("", r",") == [""]
end

@testset "split by Regex — limit kwarg (Issue #10176)" begin
    @test split("a,b,,c", r","; limit=2) == ["a", "b,,c"]
    @test split("1,2,3,4", r","; limit=3) == ["1", "2", "3,4"]
    @test split("axbyc", r"[xy]"; limit=2) == ["a", "byc"]
end

@testset "split by Regex — keepempty kwarg (Issue #10176)" begin
    @test split("a,b,,c", r","; keepempty=false) == ["a", "b", "c"]
    @test split(",a,,b,", r",") == ["", "a", "", "b", ""]
    @test split(",a,,b,", r","; keepempty=false) == ["a", "b"]
end

@testset "split by Regex — SubString element type and UTF-8 (Issues #10176, #10953)" begin
    @test SubString === Base.SubString
    @test isdefined(Base, :SubString)
    @test typeof(split("a,b", r",")) == Vector{SubString{String}}
    @test split("éà,ü", r",") == ["éà", "ü"]
end

true
