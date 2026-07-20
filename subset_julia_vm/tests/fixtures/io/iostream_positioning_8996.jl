# IO streams support cursor movement and single-character reads.

using Test

@testset "IOStream positioning" begin
    path = tempname()

    io = open(path, "w")
    @test write(io, "hello") == 5
    @test flush(io) === nothing
    close(io)

    io2 = open(path)
    @test position(io2) == 0
    @test eof(io2) == false

    @test seek(io2, 1) === io2
    @test position(io2) == 1
    @test read(io2, Char) == 'e'
    @test position(io2) == 2

    @test skip(io2, 2) === io2
    @test position(io2) == 4
    @test read(io2, Char) == 'o'
    @test eof(io2) == true

    close(io2)
    rm(path; force=true)
end

@testset "IOBuffer positioning" begin
    io = IOBuffer("hello")

    @test position(io) == 0
    @test eof(io) == false
    @test read(io, Char) == 'h'
    @test position(io) == 1

    @test seek(io, 1) === io
    @test skip(io, 2) === io
    @test position(io) == 3
    @test read(io, Char) == 'l'
    @test eof(io) == false
    @test read(io, Char) == 'o'
    @test eof(io) == true

    @test seek(io, -1) === io
    @test position(io) == 0
    @test skip(io, 99) === io
    @test position(io) == 5

    io2 = IOBuffer()
    @test write(io2, "abc") == 3
    @test position(io2) == 3
    @test eof(io2) == true
    @test seek(io2, 1) === io2
    @test write(io2, "X") == 1
    @test String(take!(io2)) == "aXc"
    @test position(io2) == 0
    @test eof(io2) == true
end

true
