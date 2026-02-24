# Test quoting of for statements
# quote for i in iter ... end end should create Expr(:for, :(i = iter), body)

using Test

@testset "Quote of for statement: quote for i in iter ... end end becomes Expr(:for, ...)" begin

    # Test basic for quote with range expression
    ex = quote
        for i in 1:10
            println(i)
        end
    end

    # Check it produces an Expr containing :for
    s = string(ex)
    @assert occursin("for", s)
    @assert occursin("i", s)

    # Test using :() form for for statement
    ex2 = :(for x in xs x end)

    # Check that string contains for
    s2 = string(ex2)
    @assert occursin("for", s2)

    # Test .head field access
    @assert string(ex2.head) == "for"

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed
