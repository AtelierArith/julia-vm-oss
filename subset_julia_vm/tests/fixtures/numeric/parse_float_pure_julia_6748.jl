# Issue #6748: parse(Float64, s) / tryparse(Float64, s) are now pure-Julia
# public functions (base/parse.jl) that call the _tryparse_float64 intrinsic
# (libc strtod). parse() raises upstream's ArgumentError on failure (the old
# Rust handler threw a generic error). parse(Int, s; base=) and string(x; base=)
# keep working. Values/exception type verified against upstream julia 1.12.

using Test

@testset "parse/tryparse Float64 (Issue #6748)" begin
    @test parse(Float64, "3.14") === 3.14
    @test parse(Float64, "1e10") === 1.0e10
    @test parse(Float64, "-2.5") === -2.5
    @test tryparse(Float64, "2.5") === 2.5
    @test tryparse(Float64, "bad") === nothing
    @test tryparse(Float64, "") === nothing
end

@testset "parse(Float64) raises ArgumentError on failure (Issue #6748)" begin
    @test_throws ArgumentError parse(Float64, "bad")
    @test_throws ArgumentError parse(Float64, "")
end

@testset "parse(Int; base=) and string(x; base=) (Issue #6748)" begin
    @test parse(Int, "ff", base=16) === 255
    @test parse(Int, "101", base=2) === 5
    @test parse(Int, "42") === 42
    @test string(255, base=16) == "ff"
    @test string(10, base=2) == "1010"
    @test tryparse(Int, "x") === nothing
    @test parse(Int, "-7") === -7
end

true
