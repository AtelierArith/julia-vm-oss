using Test

# Reproduces Issue #11386: a callable struct whose call overload uses a
# vararg parameter omitted the bound `self` argument during runtime
# dispatch, raising a MethodError, while the fixed-arity form worked.
mutable struct Box11386
    value::Int
end

mutable struct FixedCallable11386
    box::Box11386
end
(callable::FixedCallable11386)(value::Int) = callable.box.value + value

mutable struct VarargCallable11386
    box::Box11386
end
(callable::VarargCallable11386)(values...) = callable.box.value + sum(values)

mutable struct KwargsVarargCallable11386
    box::Box11386
end
(callable::KwargsVarargCallable11386)(values...; scale::Int = 1) =
    callable.box.value + scale * sum(values)

# Reproduces Issue #11553: an anonymous-form callable struct whose vararg
# overload's first FIXED parameter happens to be annotated with the struct's
# own type. Bound-ness must be decided structurally (whether the definition
# was written `(self::Type)(...)`), never by comparing a parameter's *type*
# against the struct's own type -- that heuristic cannot tell this genuinely
# anonymous method apart from a bound-form `(self::Type)(xs...)` method,
# since both end up with a first parameter typed `Type` ahead of a vararg
# tail. No receiver should ever be prepended here.
struct AnonymousSelfTypedVararg11553
    tag::Int
end
(::AnonymousSelfTypedVararg11553)(x::AnonymousSelfTypedVararg11553, xs...) =
    (x.tag, length(xs))

@testset "callable struct vararg self-binding (Issue #11386)" begin
    fixed = FixedCallable11386(Box11386(45))
    @test fixed(1) == 46

    vararg = VarargCallable11386(Box11386(45))
    @test vararg(1) == 46
    @test vararg(1, 2, 3) == 51

    args = (1, 2, 3)
    @test vararg(args...) == 51

    kw = KwargsVarargCallable11386(Box11386(45))
    @test kw(1, 2; scale = 2) == 51
    @test kw(1, 2) == 48
end

@testset "anonymous callable struct with self-typed first vararg param (Issue #11553)" begin
    a1 = AnonymousSelfTypedVararg11553(1)
    a2 = AnonymousSelfTypedVararg11553(2)
    @test a1(a2) == (2, 0)
    @test a1(a2, 10, 20) == (2, 2)
end

true
