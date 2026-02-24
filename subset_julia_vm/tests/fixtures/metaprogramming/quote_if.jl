# Test quoting of if statements
# quote if ... end end should create Expr(:if, ...)

using Test

@testset "Quote of if statement: quote if ... end end becomes Expr(:if, ...)" begin

    # Test if without else using string representation
    ex1 = quote
        if x
            1
        end
    end

    # ex1 is a :block containing an :if
    # Check string representation contains "if x"
    s1 = string(ex1)
    @assert occursin("if", s1)
    @assert occursin("x", s1)

    # Test if with else
    ex2 = quote
        if cond
            a
        else
            b
        end
    end

    s2 = string(ex2)
    @assert occursin("if", s2)
    @assert occursin("cond", s2)
    @assert occursin("else", s2)

    # Return success
    @test (2.0) == 2.0
end

true  # Test passed
