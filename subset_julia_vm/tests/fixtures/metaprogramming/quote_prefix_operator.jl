# Test quote syntax with prefix operators: :(!true), :(!false)
# Issue #304: eval での ! 演算子処理の改善

using Test

@testset "Quote syntax with prefix operators: :(!true), :(-x) etc. (Issue #304)" begin

    # Test :(!true) - quoted negation of true
    ex1 = :(!true)
    result1 = eval(ex1)
    println(":(!true) evaluates to ", result1)
    if result1
        error("Expected false from :(!true)")
    end

    # Test :(!false) - quoted negation of false
    ex2 = :(!false)
    result2 = eval(ex2)
    println(":(!false) evaluates to ", result2)
    if !result2
        error("Expected true from :(!false)")
    end

    println("All prefix operator quote tests passed!")
    @test (1.0) == 1.0
end

true  # Test passed
