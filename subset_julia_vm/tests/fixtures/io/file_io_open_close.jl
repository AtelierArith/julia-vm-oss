# Test file I/O operations: open, close, isopen, eof, readline

using Test

# Create a temporary test file
test_content = "Line 1\nLine 2\nLine 3"
tempfile = tempname()
touch(tempfile)

# Write test content to file
io = open(tempfile, "w")
write(io, test_content)
close(io)

@testset "File I/O operations" begin
    @testset "open and close" begin
        io = open(tempfile, "r")
        @test isopen(io) == true
        close(io)
        @test isopen(io) == false
    end

    @testset "readline from file handle" begin
        io = open(tempfile, "r")
        line1 = readline(io)
        @test line1 == "Line 1"
        line2 = readline(io)
        @test line2 == "Line 2"
        line3 = readline(io)
        @test line3 == "Line 3"
        close(io)
    end

    @testset "eof check" begin
        io = open(tempfile, "r")
        @test eof(io) == false
        readline(io)
        readline(io)
        readline(io)
        @test eof(io) == true
        close(io)
    end
end

# Cleanup
rm(tempfile)

true
