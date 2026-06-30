using Test

# Issue #5766: parse(Bool, s) / tryparse(Bool, s). Accepts "true"/"false"
# (whitespace-stripped) or an integer 0/1; anything else is invalid.

@testset "parse(Bool, s) (Issue #5766)" begin
    # true/false literals
    @test parse(Bool, "true") === true
    @test parse(Bool, "false") === false

    # Whitespace is stripped
    @test parse(Bool, " true ") === true
    @test parse(Bool, "  false  ") === false

    # Integer 0/1
    @test parse(Bool, "1") === true
    @test parse(Bool, "0") === false
    @test parse(Bool, "00") === false

    # tryparse returns the value or nothing
    @test tryparse(Bool, "true") === true
    @test tryparse(Bool, "false") === false
    @test tryparse(Bool, "xyz") === nothing
    @test tryparse(Bool, "2") === nothing
    @test tryparse(Bool, "True") === nothing   # case-sensitive

    # parse throws on invalid input
    @test_throws ArgumentError parse(Bool, "yes")
    @test_throws ArgumentError parse(Bool, "2")
    @test_throws ArgumentError parse(Bool, "True")
end

true
