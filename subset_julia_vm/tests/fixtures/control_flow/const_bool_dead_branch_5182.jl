# Dead-branch elimination for const-foldable Bool conditions (Issue #5182).
#
# Conditions like `if 1 < 2`, `if true && false`, or `if !false` are folded to a
# statically-known Bool at compile time, so only the live branch is compiled.
# The observable result must stay identical to evaluating the condition at runtime.

using Test

# Constant comparison: `1 < 2` is always true -> then-branch taken.
function always_true_cmp()
    if 1 < 2
        return 10
    else
        return 20
    end
end

# Constant comparison: `1 > 2` is always false -> else-branch taken.
function always_false_cmp()
    if 1 > 2
        return 10
    else
        return 20
    end
end

# Boolean algebra: `true && false` is false -> else-branch taken.
function const_and()
    if true && false
        return 1
    else
        return 2
    end
end

# Boolean algebra: `false || true` is true -> then-branch taken.
function const_or()
    if false || true
        return 1
    else
        return 2
    end
end

# Unary not: `!false` is true -> then-branch taken.
function const_not()
    if !false
        return 100
    end
    return 0
end

# Always-false with no else branch: the if contributes nothing.
function const_false_no_else()
    acc = 5
    if 2 == 3
        acc = 999
    end
    return acc
end

# Nested const expression inside a statement-position if.
function nested_const_branch()
    total = 0
    if (1 + 1) < 3 && 2 * 2 == 4
        total += 7
    else
        total += 1
    end
    return total
end

@testset "const-bool dead-branch elimination" begin
    @test always_true_cmp() == 10
    @test always_false_cmp() == 20
    @test const_and() == 2
    @test const_or() == 1
    @test const_not() == 100
    @test const_false_no_else() == 5
    @test nested_const_branch() == 7
end

true
