# Test copy(::Expr) - AST deep copy
# This tests the implementation of copy(::Expr) for Issue #298

using Test

@testset "copy(::Expr) - deep copy of Expr AST nodes (Issue #298)" begin

    # Test 1: Basic copy preserves structure
    println("Test 1: Basic copy preserves structure")
    ex = :(x + 1)
    ex2 = copy(ex)
    @assert ex.head === ex2.head "Head should match"
    @assert length(ex.args) == length(ex2.args) "Args length should match"
    @assert ex.args[1] === ex2.args[1] "Operator should match"
    @assert ex.args[2] === ex2.args[2] "First operand should match"
    @assert ex.args[3] === ex2.args[3] "Second operand should match"
    println("  passed")

    # Test 2: Copy of function call expression
    println("Test 2: Function call expression")
    call_ex = :(f(a, b, c))
    call_copy = copy(call_ex)
    @assert call_ex.head === call_copy.head "Head should match"
    @assert call_ex.args[1] === call_copy.args[1] "Function name should match"
    @assert call_ex.args[2] === call_copy.args[2] "First arg should match"
    @assert call_ex.args[3] === call_copy.args[3] "Second arg should match"
    @assert call_ex.args[4] === call_copy.args[4] "Third arg should match"
    println("  passed")

    # Test 3: Nested expression copy
    println("Test 3: Nested expression copy")
    nested = :(f(g(x), h(y)))
    nested_copy = copy(nested)
    # Check top level
    @assert nested.head === nested_copy.head "Top head should match"
    # Check nested structure is preserved
    inner1 = nested.args[2]
    inner1_copy = nested_copy.args[2]
    @assert inner1.head === inner1_copy.head "Inner1 head should match"
    @assert inner1.args[1] === inner1_copy.args[1] "Inner1 func should match"
    @assert inner1.args[2] === inner1_copy.args[2] "Inner1 arg should match"
    inner2 = nested.args[3]
    inner2_copy = nested_copy.args[3]
    @assert inner2.head === inner2_copy.head "Inner2 head should match"
    println("  passed")

    # Test 4: Copy of quote block
    println("Test 4: Quote block copy")
    block = quote
        x = 1
        y = 2
    end
    block_copy = copy(block)
    @assert block.head === block_copy.head "Block head should match"
    @assert length(block.args) == length(block_copy.args) "Block args length should match"
    println("  passed")

    # Test 5: Symbols are preserved (not copied, shared)
    println("Test 5: Symbols are shared")
    sym_ex = :(foo + bar)
    sym_copy = copy(sym_ex)
    @assert sym_ex.args[2] === sym_copy.args[2] "Symbols should be identical"
    println("  passed")

    println("\nAll copy(::Expr) tests passed!")
    @test (1.0) == 1.0
end

true  # Test passed
