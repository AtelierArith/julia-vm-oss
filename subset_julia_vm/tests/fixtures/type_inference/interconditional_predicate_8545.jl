# InterConditional: user predicates narrow at the call site (Issue #8545).
#
# A single-argument predicate whose body reduces to a conditional on its
# argument slot is translated into the caller's frame (upstream
# `InterConditional` / `from_interconditional`), so `if isint(x)` narrows `x`
# exactly like an inline `x isa Int`.
using Test

isint_8545(x) = x isa Int64
isnil_8545(x) = isnothing(x)
ispositive_int_8545(x) = x isa Int64 && x > 0
isstr_or_nothing_8545(x) = x isa String || x === nothing

function call_isint_8545(x::Union{Int64,String})
    if isint_8545(x)
        return x + 1
    end
    return 0
end

# Predicate delegating to another predicate (nested translation).
function call_isnil_8545(x::Union{Int64,Nothing})
    isnil_8545(x) && return 0
    x + 1
end

# Compound predicate body: `x isa Int64 && x > 0` still narrows the
# then-branch to Int64 (the non-narrowing `x > 0` leaf is neutral).
function call_ispositive_8545(x::Union{Int64,Nothing})
    if ispositive_int_8545(x)
        return x + 1
    end
    return 0
end

# `||` predicate body used as an early-return guard.
function call_isstr_or_nothing_8545(x::Union{Int64,String,Nothing})
    isstr_or_nothing_8545(x) && return 0
    x + 1
end

@testset "InterConditional predicate behavior (Issue #8545)" begin
    @test call_isint_8545(41) == 42
    @test call_isint_8545("s") == 0
    @test call_isnil_8545(41) == 42
    @test call_isnil_8545(nothing) == 0
    @test call_ispositive_8545(41) == 42
    @test call_ispositive_8545(-1) == 0
    @test call_ispositive_8545(nothing) == 0
    @test call_isstr_or_nothing_8545(41) == 42
    @test call_isstr_or_nothing_8545("s") == 0
    @test call_isstr_or_nothing_8545(nothing) == 0
end

@testset "InterConditional predicate inference (Issue #8545)" begin
    @test Base.infer_return_type(call_isint_8545, Tuple{Union{Int64,String}}) === Int64
    @test Base.infer_return_type(call_isnil_8545, Tuple{Union{Int64,Nothing}}) === Int64
    @test Base.infer_return_type(call_ispositive_8545, Tuple{Union{Int64,Nothing}}) === Int64
    @test Base.infer_return_type(call_isstr_or_nothing_8545, Tuple{Union{Int64,String,Nothing}}) === Int64
end

true
