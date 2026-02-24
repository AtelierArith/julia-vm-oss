# Test Meta operator predicate functions:
# Meta.isoperator, Meta.isunaryoperator, Meta.isbinaryoperator, Meta.ispostfixoperator

using Test

@testset "Meta.isoperator" begin
    # Common arithmetic operators
    @assert Meta.isoperator(:+)
    @assert Meta.isoperator(:-)
    @assert Meta.isoperator(:*)
    @assert Meta.isoperator(:/)
    @assert Meta.isoperator(:^)

    # Comparison operators
    @assert Meta.isoperator(Symbol("=="))
    @assert Meta.isoperator(Symbol("!="))
    @assert Meta.isoperator(:<)
    @assert Meta.isoperator(:>)

    # Postfix operator (transpose)
    @assert Meta.isoperator(Symbol("'"))

    # Not operators: identifiers
    @assert !Meta.isoperator(:x)
    @assert !Meta.isoperator(:foo)
    @assert !Meta.isoperator(:hello)

    @test true
end

@testset "Meta.isunaryoperator" begin
    # Unary plus and minus
    @assert Meta.isunaryoperator(:+)
    @assert Meta.isunaryoperator(:-)

    # Logical not
    @assert Meta.isunaryoperator(:!)

    # Bitwise not
    @assert Meta.isunaryoperator(:~)

    # Binary-only operators are NOT unary
    @assert !Meta.isunaryoperator(:*)
    @assert !Meta.isunaryoperator(:/)
    @assert !Meta.isunaryoperator(:^)
    @assert !Meta.isunaryoperator(Symbol("=="))

    # Identifiers are not unary operators
    @assert !Meta.isunaryoperator(:x)
    @assert !Meta.isunaryoperator(:foo)

    @test true
end

@testset "Meta.isbinaryoperator" begin
    # Common binary operators
    @assert Meta.isbinaryoperator(:+)
    @assert Meta.isbinaryoperator(:-)
    @assert Meta.isbinaryoperator(:*)
    @assert Meta.isbinaryoperator(:/)
    @assert Meta.isbinaryoperator(:^)

    # Comparison operators
    @assert Meta.isbinaryoperator(Symbol("=="))
    @assert Meta.isbinaryoperator(:<)
    @assert Meta.isbinaryoperator(:>)

    # Identifiers are not binary operators
    @assert !Meta.isbinaryoperator(:x)
    @assert !Meta.isbinaryoperator(:foo)

    @test true
end

@testset "Meta.ispostfixoperator" begin
    # Transpose/adjoint operator is postfix
    @assert Meta.ispostfixoperator(Symbol("'"))

    # Common operators are not postfix
    @assert !Meta.ispostfixoperator(:+)
    @assert !Meta.ispostfixoperator(:-)
    @assert !Meta.ispostfixoperator(:!)
    @assert !Meta.ispostfixoperator(:x)

    @test true
end

true
