# Issue #5180: value-position if/ternary/begin should type-join their branch
# expressions instead of widening to Any.
#
# `infer_expr_type` previously fell through to ValueType::Any for Expr::Ternary
# and Expr::LetBlock (value-position `if`/`begin` are lowered to Ternary/LetBlock).
# That dropped slot typing when such an expression is used directly as an array
# index or operand. These fixtures lock in correctness + upstream-Julia parity:
# same-typed branches keep their concrete type, mixed I64/F64 branches stay
# correct per the branch taken, and incompatible branches stay dynamic.

using Test

# Ternary used directly as an index into a typed Int64 array. When both branches
# infer the same concrete I64 type the index/result stay I64-typed.
function ternary_index_same_type(cond, a, b, c, d)
    arr = [10, 20, 30, 40, 50]
    return arr[cond ? a + b : c + d]
end

# if/else in value position (lowered to Ternary) used directly as an index.
function if_value_index(cond)
    arr = [7, 8, 9]
    return arr[cond ? 1 : 3]
end

# begin/end block (lowered to LetBlock) in operand position.
function begin_block_operand(n)
    y = (begin
        t = n + 1
        t
    end) + 100
    return y
end

# if/else value-position block as an operand (lowered to Ternary).
function if_block_operand(cond)
    y = (cond ? 10 : 20) * 2
    return y
end

# Same-typed ternary as an arithmetic operand keeps I64 result.
function ternary_operand_same_type(cond)
    x = (cond ? 10 : 20) + 5
    return x
end

# Mixed I64/F64 branches: the ternary returns whichever branch is taken (no
# runtime promotion). typeof tracks the branch.
function ternary_mixed(cond)
    return cond ? 1 : 2.0
end

# Incompatible branches (Int vs String): value semantics must follow the branch.
function ternary_incompatible(cond)
    return cond ? 1 : "s"
end

@testset "ternary/if value-position used as typed index" begin
    @test ternary_index_same_type(true, 1, 1, 5, 5) == 20
    @test ternary_index_same_type(false, 1, 1, 2, 1) == 30
    @test if_value_index(true) == 7
    @test if_value_index(false) == 9
end

@testset "begin/if/ternary value-position as operand" begin
    @test begin_block_operand(0) == 101
    @test begin_block_operand(2) == 103
    @test if_block_operand(true) == 20
    @test if_block_operand(false) == 40
    @test ternary_operand_same_type(true) == 15
    @test ternary_operand_same_type(false) == 25
    @test ternary_operand_same_type(true) isa Int
end

@testset "ternary mixed/incompatible branches match Julia" begin
    @test ternary_mixed(true) === 1
    @test ternary_mixed(false) === 2.0
    @test typeof(ternary_mixed(true)) === Int
    @test typeof(ternary_mixed(false)) === Float64
    @test ternary_incompatible(true) === 1
    @test ternary_incompatible(false) === "s"
end

true
