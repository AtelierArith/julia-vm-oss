using Test

@testset "print(io, ::Symbol) writes bare name (Issue #4741)" begin
    io = IOBuffer()
    print(io, :foo)
    @test String(take!(io)) == "foo"

    io2 = IOBuffer()
    print(io2, :hello, " ", :world)
    @test String(take!(io2)) == "hello world"

    # show still uses the show-form with ':' prefix.
    io3 = IOBuffer()
    show(io3, :foo)
    @test String(take!(io3)) == ":foo"
end

@testset "println(io, ::Symbol) writes bare name with newline (Issue #4741)" begin
    io = IOBuffer()
    println(io, :foo)
    @test String(take!(io)) == "foo\n"
end

@testset "string interpolation of Symbol drops ':' (Issue #4741)" begin
    s = :bar
    @test "got $s" == "got bar"
    @test "$(:baz) here" == "baz here"
end

@testset "string(::Symbol) stays bare (Issue #4741)" begin
    # string() was already correct before this PR; assert it stays so.
    @test string(:qux) == "qux"
end

true
