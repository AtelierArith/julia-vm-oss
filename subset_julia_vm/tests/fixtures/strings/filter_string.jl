# filter(pred, s::String) returns String, not Vector{Char} (Issue #2062)

using Test

@testset "filter(::Function, ::String) returns String" begin
    @test filter(isletter, "h3ll0 w0rld") == "hllwrld"
    @test filter(isdigit, "h3ll0 w0rld") == "30"
    @test filter(isspace, "hello world") == " "
    @test filter(isletter, "123") == ""
    @test filter(isletter, "") == ""
    @test filter(isletter, "abc") == "abc"
end

true
