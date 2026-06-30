using Test

# Issue #5056: user-defined types with a MULTI-LEVEL abstract ancestry must
# resolve the full supertype chain — for `supertype`/`<:`/`isa` *and* for
# multiple dispatch, including chains that pass through one or more user
# abstract types before reaching a built-in abstract type (`Number`).

# Pure user-defined 3-level abstract chain, bottoming out at `Any`.
abstract type A end
abstract type B <: A end
abstract type C <: B end
struct D <: C end

# Chain that bottoms out in a built-in abstract type (`Number`) only after
# passing through two user abstract types.
abstract type MyNum <: Number end
abstract type MyInt <: MyNum end
struct Tiny <: MyInt
    v::Int
end

# Methods defined at each level of the hierarchy.
ftop(x::A) = "A-method"
fmid(x::B) = "B-method"
fnum(x::Number) = "Number-method"
fmynum(x::MyNum) = "MyNum-method"

@testset "multilevel ancestor chain (Issue #5056)" begin
    # supertype walks one declared level at a time.
    @test supertype(D) === C
    @test supertype(C) === B
    @test supertype(B) === A
    @test supertype(A) === Any
    @test supertype(MyInt) === MyNum
    @test supertype(MyNum) === Number

    # Transitive subtype through the user-defined chain.
    @test D <: A
    @test D <: B
    @test D <: C
    @test C <: A
    @test B <: A

    # Transitive subtype through user abstracts down to a built-in abstract.
    @test Tiny <: MyInt
    @test Tiny <: MyNum
    @test Tiny <: Number

    # isa walks the full ancestry.
    d = D()
    @test d isa A
    @test d isa B
    @test d isa C
    @test d isa D

    t = Tiny(7)
    @test t isa MyInt
    @test t isa MyNum
    @test t isa Number

    # Dispatch to a method defined on the TOP user abstract `A` from a `D`.
    @test ftop(d) == "A-method"
    # Dispatch to a method on an intermediate user abstract `B`.
    @test fmid(d) == "B-method"
    # Dispatch to a method on the user abstract `MyNum` from a `Tiny`.
    @test fmynum(t) == "MyNum-method"
    # Dispatch to a method on the built-in abstract `Number`, reached only
    # through the user abstracts `MyInt`/`MyNum` (the Issue #5056 gap).
    @test fnum(t) == "Number-method"
end

true
