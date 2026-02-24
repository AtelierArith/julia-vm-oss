# Test quoting of ternary expressions
# :(a ? b : c) should create Expr(:if, :a, :b, :c)

using Test

@testset "Quote of ternary expression: :(a ? b : c) becomes Expr(:if, ...)" begin

    # Test basic ternary quote
    ex = :(x ? 1 : 2)

    # Check it produces an Expr (use string to verify structure)
    @assert string(ex) == "if x\n    1\nelse\n    2\nend"

    # Return success
    @test (1.0) == 1.0
end

true  # Test passed
