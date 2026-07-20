# Issue #8861: a package/user module can legitimately declare its own
# `abstract type Set end`, shadowing `Base.Set` for unqualified references
# in that scope (a normal Julia idiom -- `AbstractAlgebra.jl` does exactly
# this). `StructHierarchy` has no module scoping, so this used to silently
# lose to Base's concrete `struct Set{T} <: AbstractSet{T}` (same bare name,
# registered first): a struct declared `<: MySpecialSet <: Set` (the user's
# own `Set`) incorrectly resolved through `Base.AbstractSet`'s `show` method
# instead of the user's own, crashing when that method tried to `iterate` a
# non-iterable value.
abstract type Set end
abstract type MySpecialSet <: Set end
struct Foo <: MySpecialSet end

Base.show(io::IO, x::Foo) = print(io, "Foo!")

ok = sprint(show, Foo()) == "Foo!" &&
     string(Foo()) == "Foo!" &&
     Foo <: MySpecialSet &&
     Foo() isa Set

println(ok)
ok
