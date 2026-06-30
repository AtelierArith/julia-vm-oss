# Issue #5181: flow-sensitive `isa` narrowing wired into branch codegen.
# Inside an `isa`-guarded then-branch the guarded variable is refined to a
# concrete type so typed loads / arithmetic specialize. These tests lock in
# that the optimization preserves Julia semantics across the tricky cases:
# reassignment inside the branch, `&&` chains, struct guards, and the negation
# (else) path that must NOT be narrowed.
using Test

# A union-ish `Any` argument narrowed to Int64 in the then-branch.
function add_if_int(x)
    if x isa Int64
        return x + x
    end
    return -1
end

# Float64 narrowing keeps float arithmetic / typeof.
function double_if_float(x)
    if x isa Float64
        return x * 2.0
    end
    return 0.0
end

# Reassigning the narrowed variable inside the branch must persist past it
# (Julia variables are function-scoped). The narrowed type must not clobber the
# reassignment's type.
function reassign_in_branch(x)
    if x isa Int64
        x = "now a string"
    end
    return x
end

# `&&` chain narrows both operands; the second guard re-narrows the same var.
function chained_guard(x)
    if x isa Int64 && x > 0
        return x * 10
    end
    return 0
end

# The else branch must keep dynamic behavior (no narrowing leaks).
function classify(x)
    if x isa Int64
        return 1
    else
        # x is still `Any` here — calling a generic op must dispatch at runtime.
        return string(x)
    end
end

# String guard narrows to a String slot.
function shout_if_string(x)
    if x isa String
        return x * "!"
    end
    return "?"
end

@testset "isa flow narrowing codegen (Issue #5181)" begin
    @test add_if_int(21) == 42
    @test add_if_int(3.5) == -1
    @test add_if_int("x") == -1

    @test double_if_float(2.5) == 5.0
    @test typeof(double_if_float(2.5)) === Float64
    @test double_if_float(3) == 0.0

    @test reassign_in_branch(7) == "now a string"
    @test reassign_in_branch(2.0) == 2.0
    @test reassign_in_branch("keep") == "keep"

    @test chained_guard(5) == 50
    @test chained_guard(-5) == 0
    @test chained_guard("x") == 0

    @test classify(9) == 1
    @test classify(2.0) == "2.0"

    @test shout_if_string("hi") == "hi!"
    @test shout_if_string(3) == "?"
end

true
