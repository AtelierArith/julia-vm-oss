# Test @macroexpand for @show macro
# Verifies that macro expansion produces correct structure

using Test

f(x) = x + 1

@testset "@macroexpand @show structure" begin
    # Get the expanded form of @show f(4)
    expanded = @macroexpand @show f(4)
    
    # The expansion should be an Expr
    @test typeof(expanded) == Expr
    
    # Convert to string for inspection
    expanded_str = string(expanded)
    
    # The expansion should contain the literal string "f(4)"
    # This verifies that string(ex) was evaluated at expansion time
    @test contains(expanded_str, "f(4)")
end

true
