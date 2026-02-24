# Test Meta.isexpr Pure Julia implementation

using Test

@testset "Meta.isexpr Pure Julia implementation - check Expr head and args length" begin

    # Basic isexpr with Symbol head
    ex1 = :(f(x))
    @assert Meta.isexpr(ex1, :call)
    @assert !Meta.isexpr(ex1, :block)

    # isexpr with length check
    @assert Meta.isexpr(ex1, :call, 2)  # head=:call, 2 args: f and x
    @assert !Meta.isexpr(ex1, :call, 1)  # wrong length
    @assert !Meta.isexpr(ex1, :call, 3)  # wrong length

    # isexpr with non-Expr input
    @assert !Meta.isexpr(:x, :call)     # Symbol, not Expr
    @assert !Meta.isexpr(42, :call)     # Int64, not Expr
    @assert !Meta.isexpr("str", :call)  # String, not Expr

    # isexpr with array of heads
    ex2 = :(begin x end)
    @assert Meta.isexpr(ex2, [:block, :call])   # head is :block
    @assert Meta.isexpr(ex1, [:block, :call])   # head is :call
    @assert !Meta.isexpr(ex1, [:if, :for])      # head is :call, not in list

    # isexpr with array of heads and length
    @assert Meta.isexpr(ex1, [:block, :call], 2)
    @assert !Meta.isexpr(ex1, [:block, :call], 1)

    # isexpr with tuple of heads
    @assert Meta.isexpr(ex1, (:block, :call))
    @assert !Meta.isexpr(ex1, (:if, :for))

    # isexpr with tuple of heads and length
    @assert Meta.isexpr(ex1, (:block, :call), 2)
    @assert !Meta.isexpr(ex1, (:block, :call), 1)

    # All tests passed
    @test (true)
end

true  # Test passed
