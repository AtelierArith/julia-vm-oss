# Test Meta.quot Pure Julia implementation

using Test

@testset "Meta.quot Pure Julia implementation - create quoted expressions" begin

    # Basic quot with symbol
    quoted_sym = Meta.quot(:x)
    @assert quoted_sym.head == :quote
    @assert length(quoted_sym.args) == 1
    @assert quoted_sym.args[1] == :x

    # quot with expression
    quoted_expr = Meta.quot(:(1 + 2))
    @assert quoted_expr.head == :quote
    @assert length(quoted_expr.args) == 1

    # quot with literal values
    quoted_int = Meta.quot(42)
    @assert quoted_int.head == :quote
    @assert quoted_int.args[1] == 42

    # Verify the quoted expression has the right structure
    inner = quoted_expr.args[1]
    @assert inner.head == :call
    @assert length(inner.args) == 3  # the operator +, and two operands 1, 2

    # All tests passed
    @test (true)
end

true  # Test passed
