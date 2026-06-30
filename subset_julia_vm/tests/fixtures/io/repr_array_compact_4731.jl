using Test

@testset "repr(::Array) is compact form, not multi-line show (Issue #4731)" begin
    @test repr([1, 2, 3]) == "[1, 2, 3]"
    @test repr([1.0, 2.0]) == "[1.0, 2.0]"
    @test repr(["a", "b"]) == "[\"a\", \"b\"]"
    @test repr([1 2; 3 4]) == "[1 2; 3 4]"
    # Empty typed-array repr intentionally diverges (sjulia returns
    # "[]", upstream returns "Int64[]"). Tracked separately, not part
    # of #4731.
end

@testset "println(io::IOBuffer) writes only a newline, no debug repr leak (Issue #4731)" begin
    io = IOBuffer()
    println(io)
    @test String(take!(io)) == "\n"

    io2 = IOBuffer()
    print(io2, "hello")
    println(io2)
    @test String(take!(io2)) == "hello\n"
end

@testset "string(::Array) compact form stays parity-correct" begin
    @test string([1, 2, 3]) == "[1, 2, 3]"
    @test string([1.5, 2.5]) == "[1.5, 2.5]"
end

true
