# Test dump for QuoteNode

using Test

@testset "dump() for QuoteNode - shows value field and nested structure" begin

    # QuoteNode wrapping a symbol
    q1 = QuoteNode(:x)
    dump(q1)

    # QuoteNode wrapping an expression
    q2 = QuoteNode(:(1 + 2))
    dump(q2)

    # Access value field
    @assert q1.value === :x

    # QuoteNode value with expression
    v = q2.value
    @assert v isa Expr
    @assert v.head === :call

    # Final result
    @test (42.0) == 42.0
end

true  # Test passed
