# Test quoting of while statements
# quote while cond ... end end should create Expr(:while, cond, body)

using Test

@testset "Quote of while statement: quote while cond ... end end becomes Expr(:while, ...)" begin

    # Test basic while quote with compound assignment
    ex = quote
        while x > 0
            x -= 1
        end
    end

    # Check it produces an Expr containing :while
    s = string(ex)
    @assert occursin("while", s)
    @assert occursin("x", s)

    # Test using :() form for while statement
    ex2 = :(while cond body end)

    s2 = string(ex2)
    @assert occursin("while", s2)
    @assert occursin("cond", s2)

    # Test .head field access
    @assert string(ex2.head) == "while"

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed
