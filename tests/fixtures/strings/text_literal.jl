# Test text"..." string literal (Issue #468)
# Tests that text"string" creates a Text{String} object

using Test

@testset "Text string literal" begin
    # Basic text literal
    t = text"hello world"
    @test isa(t, Text{String})
    @test t.content == "hello world"

    # Text with special characters
    t2 = text"line1\nline2"
    @test isa(t2, Text{String})

    # Empty text
    t3 = text""
    @test isa(t3, Text{String})
    @test t3.content == ""

    # Text equality
    @test text"test" == text"test"
    @test !(text"a" == text"b")
end

true
