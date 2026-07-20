# A `const` type alias used as a PARAMETER ANNOTATION must dispatch (Issue #11104).
#
# `const AE = E; f(x::AE) = 6` registered the method against the nominal
# placeholder `Struct("AE")` — a type no value is ever an instance of — so
# `f(E())` raised `MethodError`, while the same alias worked as a
# value/constructor. Root cause: the type-alias detection gate only recognized
# BUILTIN type names on the RHS of a binding, so an alias of a user-declared
# type was never registered in the alias table and the signature annotation had
# nothing to expand. Fix: the lowering pre-scan now also collects the type names
# the program DECLARES (`struct` / `mutable struct` / `abstract type` /
# `primitive type`, including inside modules) and alias chains, so `const AE = E`
# registers and `parse_type_name` expands `AE` -> `E`.
#
# All expectations verified against upstream Julia 1.12.

using Test

struct E11104 end
abstract type MyAbs11104 end
struct D11104 <: MyAbs11104 end
mutable struct Mut11104
    y::Int
end
struct Box11104{T}
    v::T
end

const AE11104 = E11104
const AAbs11104 = MyAbs11104
const AMut11104 = Mut11104
const MyInt11104 = Int64
const MyNum11104 = Real
const VecOf11104 = Vector{Int64}
const BoxInt11104 = Box11104{Int64}
V11104{T} = Vector{T}
const AliasOfAlias11104 = AE11104

f_struct(x::AE11104) = 6
f_abstract(x::AAbs11104) = 7
f_mutable(x::AMut11104) = x.y
f_builtin(x::MyInt11104) = 8
f_number(x::MyNum11104) = 9
f_vec(x::VecOf11104) = length(x)
f_boxint(x::BoxInt11104) = x.v
f_param_applied(x::V11104{Int64}) = 20
f_param_bare(x::V11104) = 21
f_chain(x::AliasOfAlias11104) = 22
f_where(x::T) where {T<:MyNum11104} = 23
f_kw(; x::MyInt11104 = 1) = x + 100

@testset "alias of a user struct / abstract / mutable type dispatches (Issue #11104)" begin
    @test f_struct(E11104()) == 6
    @test f_abstract(D11104()) == 7
    @test f_mutable(Mut11104(5)) == 5
    @test f_chain(E11104()) == 22
end

@testset "alias of a builtin type dispatches (Issue #11104)" begin
    @test f_builtin(3) == 8
    @test f_number(1) == 9
    @test f_number(1.5) == 9
    @test f_vec([1, 2, 3]) == 3
    @test f_boxint(Box11104(4)) == 4
end

@testset "parametric alias annotations (Issue #11104)" begin
    @test f_param_applied([1, 2]) == 20
    @test f_param_bare([1.0]) == 21
end

@testset "alias in a where bound and in a keyword annotation (Issue #11104)" begin
    @test f_where(1) == 23
    @test f_where(2.5) == 23
    @test f_kw() == 101
    @test f_kw(x = 5) == 105
end

@testset "the alias still works as a value / constructor (Issue #11104)" begin
    @test AE11104 === E11104
    @test AE11104() isa E11104
    @test AMut11104(2).y == 2
    @test isa(E11104(), AE11104)
    @test BoxInt11104 === Box11104{Int64}
end

module Mod11104
struct Inner end
abstract type InnerAbs end
struct InnerSub <: InnerAbs end
const AInner = Inner
const AInnerAbs = InnerAbs
inner_id(x::AInner) = 42
inner_abs_id(x::AInnerAbs) = 43
struct ParametricInner11104{T}
    x::T
    ParametricInner11104{T}(x::AInner) where T = (:inner, x)
end
make_parametric_inner() = ParametricInner11104{Inner}(Inner())
end

@testset "alias declared and used inside a module (Issue #11104)" begin
    @test Mod11104.inner_id(Mod11104.Inner()) == 42
    @test Mod11104.inner_abs_id(Mod11104.InnerSub()) == 43
    result = Mod11104.make_parametric_inner()
    @test result[1] == :inner
    @test result[2] isa Mod11104.Inner
end

true
