# Issue #5493: arity validation for the type-operator builtins <: / >: / isa.
#
# Upstream Julia (julia/src/builtins.c) validates these builtins with
# `JL_NARGS(<:, 2, 2)` / `JL_NARGS(isa, 2, 2)`, raising an ArgumentError
# ("<:: too few arguments (expected 2)" / "isa: too many arguments
# (expected 2)") on the wrong arity. `>:` is an ordinary 2-arg function
# (julia/base/operators.jl: `>:(a, b) = (b <: a)`), so a wrong-arity call
# raises a MethodError instead.
#
# Before this fix the VM's immediate builtin path assumed a fixed arity of 2:
#   - too-few  ((<:)(Number))           -> uncatchable "Stack underflow"
#   - too-many ((<:)(Int, Number, Bool)) -> silently returned a Bool
# Both diverge from upstream, which raises a catchable ArgumentError.
using Test

# Bind the operators to plain variables so the @test_throws macro receives a
# simple call expression (matching the first-class usage exercised by #5115).
const lt = (<:)
const gt = (>:)
const is_a = isa

@testset "type-operator builtin arity (Issue #5493)" begin
    # `<:` — builtin, ArgumentError on wrong arity (matches upstream).
    @test_throws ArgumentError lt(Number)
    @test_throws ArgumentError lt(Int, Number, Bool)

    # `isa` — builtin, ArgumentError on wrong arity (matches upstream).
    @test_throws ArgumentError is_a(1)
    @test_throws ArgumentError is_a(1, Int, Bool)

    # `>:` — ordinary function, MethodError on wrong arity (matches upstream).
    @test_throws MethodError gt(Number)
    @test_throws MethodError gt(Int, Number, Bool)
end

# Regression guards: the wrong-arity calls must raise a CATCHABLE exception
# (the too-few case used to abort with an uncatchable "Stack underflow", and
# the too-many case used to silently return a Bool rather than throwing).
function lt_too_few_is_catchable()
    try
        lt(Number)
        return false
    catch
        return true
    end
end

function lt_too_many_is_catchable()
    try
        lt(Int, Number, Bool)
        return false
    catch
        return true
    end
end

function isa_too_few_is_catchable()
    try
        is_a(1)
        return false
    catch
        return true
    end
end

function isa_too_many_is_catchable()
    try
        is_a(1, Int, Bool)
        return false
    catch
        return true
    end
end

@testset "wrong-arity type-operator calls are catchable (Issue #5493)" begin
    @test lt_too_few_is_catchable()
    @test lt_too_many_is_catchable()
    @test isa_too_few_is_catchable()
    @test isa_too_many_is_catchable()

    # The correct 2-arg forms must still work unchanged.
    @test lt(Int, Number) == true
    @test lt(Number, Int) == false
    @test is_a(3, Int) == true
    @test is_a(3, String) == false
    @test gt(Number, Int) == true
end

println("all 5493 checks passed")
true
