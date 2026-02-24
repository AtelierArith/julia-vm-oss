# Test error() function
# error() throws an ErrorException with a given message

using Test

@testset "error() function" begin
    # error(msg::AbstractString) - throws ErrorException with message
    caught_len = 0
    try
        error("something went wrong")
    catch e
        caught_len = length(e.msg)
    end
    @test caught_len == 20  # "something went wrong" has 20 chars

    # error() - throws empty ErrorException
    empty_len = -1
    try
        error()
    catch e
        empty_len = length(e.msg)
    end
    @test empty_len == 0

    # error(a, b) - concatenates arguments
    multi_len = 0
    try
        error("x = ", 42)
    catch e
        multi_len = length(e.msg)
    end
    @test multi_len == 6  # "x = 42" has 6 chars

    # error(a, b, c) - three arguments
    three_len = 0
    try
        error("a", "b", "c")
    catch e
        three_len = length(e.msg)
    end
    @test three_len == 3  # "abc" has 3 chars
end

true
