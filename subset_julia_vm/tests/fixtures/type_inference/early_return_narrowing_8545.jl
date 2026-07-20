# Flow-sensitive early-return narrowing (Issue #8545).
#
# When one arm of a branch terminates (return), the NEGATED condition must
# apply to the fall-through state instead of joining both arms, so guards
# like `isnothing(x) && return 0` narrow `x` for the rest of the function.
using Test

# `cond && return` guard: fall-through implies the guard condition was false.
function guard_and_return_8545(x::Union{Int64,Nothing})
    isnothing(x) && return 0
    x + 1
end

# `cond || return` guard: fall-through implies the guard condition was true.
function guard_or_return_8545(x::Union{Int64,Nothing})
    x isa Int64 || return 0
    x + 1
end

# Plain `if` whose then-branch unconditionally returns: the post-if state is
# the else split (x narrowed to Int64), not the join of both arms.
function if_return_tail_8545(x::Union{Int64,Nothing})
    if x === nothing
        return 0
    end
    return x + 1
end

# Narrowing survives >= 2 successive guard blocks: each guard removes one
# union member from the fall-through state.
function chained_guards_8545(x::Union{Int64,Nothing,String})
    isnothing(x) && return -1
    x isa String && return -2
    x + 1
end

# Narrowing survives inside a nested block: the inner guard narrows the
# remainder of the outer then-branch.
function nested_guard_8545(x::Union{Int64,Nothing}, flag::Bool)
    if flag
        isnothing(x) && return 0
        return x + 1
    end
    return -1
end

@testset "early-return narrowing behavior (Issue #8545)" begin
    @test guard_and_return_8545(41) == 42
    @test guard_and_return_8545(nothing) == 0
    @test guard_or_return_8545(41) == 42
    @test guard_or_return_8545(nothing) == 0
    @test if_return_tail_8545(41) == 42
    @test if_return_tail_8545(nothing) == 0
    @test chained_guards_8545(41) == 42
    @test chained_guards_8545(nothing) == -1
    @test chained_guards_8545("s") == -2
    @test nested_guard_8545(41, true) == 42
    @test nested_guard_8545(nothing, true) == 0
    @test nested_guard_8545(nothing, false) == -1
end

@testset "early-return narrowing inference (Issue #8545)" begin
    # The fall-through `x + 1` sees the narrowed Int64, so the whole function
    # infers Int64 (no Union/Any dynamic fallback).
    @test Base.infer_return_type(guard_and_return_8545, Tuple{Union{Int64,Nothing}}) === Int64
    @test Base.infer_return_type(guard_or_return_8545, Tuple{Union{Int64,Nothing}}) === Int64
    @test Base.infer_return_type(if_return_tail_8545, Tuple{Union{Int64,Nothing}}) === Int64
    @test Base.infer_return_type(chained_guards_8545, Tuple{Union{Int64,Nothing,String}}) === Int64
    @test Base.infer_return_type(nested_guard_8545, Tuple{Union{Int64,Nothing},Bool}) === Int64
end

true
