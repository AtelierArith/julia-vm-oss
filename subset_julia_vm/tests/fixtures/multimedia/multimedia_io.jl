# Test multimedia I/O functions (Issue #455)
# Tests showable, displayable

using Test

@testset "showable function" begin
    # text/plain is always showable for any value
    @test showable("text/plain", 42) == true
    @test showable("text/plain", "hello") == true

    # Other MIME types default to false
    @test showable("image/png", 42) == false
end

@testset "displayable with String" begin
    # String overloads
    @test displayable("text/plain") == true
    @test displayable("text/html") == true
    @test displayable("image/png") == false
    @test displayable("application/json") == true
end

@testset "displayable with MIME constructor" begin
    # Direct MIME type argument
    @test displayable(MIME("text/plain")) == true
    @test displayable(MIME("image/png")) == false
end

@testset "displayable with TextDisplay" begin
    td = TextDisplay(stdout)
    @test displayable(td, "text/plain") == true
    @test displayable(td, MIME("text/html")) == true
end

true
