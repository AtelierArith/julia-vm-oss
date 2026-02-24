using Test

# IOContext with sprint: verifying context-aware formatting
@testset "IOContext context-aware sprint" begin
    # sprint without context returns string representation
    s1 = sprint(print, 42)
    @test s1 == "42"

    # sprint with multiple values
    s2 = sprint(print, "x=", 10)
    @test s2 == "x=10"

    # IOContext wraps an IO stream (no errors creating it)
    buf = IOBuffer()
    ctx = IOContext(buf, :compact => true)
    @test ctx !== nothing

    # Writing to IOContext through print
    print(ctx, "hello")
    @test String(take!(buf)) == "hello"
end

true
