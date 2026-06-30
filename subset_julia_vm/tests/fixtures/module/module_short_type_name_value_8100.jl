# Regression test for Issue #8100: a module-private type whose name matches the
# single-letter / single-letter+digits type-variable spelling (`E`, `T1`), when
# referenced AS A VALUE inside its module, must resolve to its real `DataType`,
# not to a `TypeVar`. Before the fix `typeof(ShortTypeNames8100.getE())` wrongly returned
# `TypeVar` because the module-private struct registers under its qualified name
# (`ShortTypeNames8100.E`) while the bare reference projects `JuliaType::Struct("E")`, which the
# string-level type-variable heuristic mistook for a free type variable. Longer
# names (`Elem`) were unaffected, so the bug was length/spelling specific.
using Test

module ShortTypeNames8100
struct E end           # 1-char, matches the type-variable spelling
struct T1 end          # uppercase + digit, also matches the spelling
struct Ab end          # 2-char, does NOT match the spelling (sanity)
struct Elem end        # long name (control)
abstract type Q end    # 1-char abstract type
getE() = (t = E; t)    # short name bound to a local, then returned as a value
getT1() = T1
getAb() = Ab
getElem() = Elem
getQ() = Q
passthrough(x) = x
useE() = passthrough(E) # short type value passed through a function
# A genuine short type PARAMETER must still behave as a type variable.
typeparam(x::T) where {T} = T
typeparam2(::Type{S}) where {S} = S
end

@testset "module-private short type name as value resolves to DataType (Issue #8100)" begin
    # typeof of a short module-private CONCRETE struct value is DataType.
    @test typeof(ShortTypeNames8100.getE()) === DataType
    @test typeof(ShortTypeNames8100.getT1()) === DataType
    @test typeof(ShortTypeNames8100.getAb()) === DataType
    @test typeof(ShortTypeNames8100.getElem()) === DataType
    # The bare type value is NOT a TypeVar / not === DataType itself.
    @test typeof(ShortTypeNames8100.getE()) !== TypeVar
    @test (ShortTypeNames8100.getE() === DataType) == false

    # The bare-returned type value is the SAME type object as the qualified name.
    @test ShortTypeNames8100.getE() === ShortTypeNames8100.E
    @test ShortTypeNames8100.getT1() === ShortTypeNames8100.T1
    @test ShortTypeNames8100.getAb() === ShortTypeNames8100.Ab
    @test ShortTypeNames8100.getElem() === ShortTypeNames8100.Elem
    @test ShortTypeNames8100.useE() === ShortTypeNames8100.E

    # A short module-private ABSTRACT type as a value is a DataType too.
    @test typeof(ShortTypeNames8100.getQ()) === DataType
    @test ShortTypeNames8100.getQ() === ShortTypeNames8100.Q

    # The short type values behave as real types.
    @test ShortTypeNames8100.getE() isa Type
    @test ShortTypeNames8100.getE() isa DataType
    @test ShortTypeNames8100.getE() <: Any
    @test isconcretetype(ShortTypeNames8100.getE())
    @test ShortTypeNames8100.getE() !== ShortTypeNames8100.Ab

    # Regression: a genuine short type PARAMETER (`where {T}`) is unaffected and
    # still resolves to the argument's concrete type, not a struct/DataType.
    @test ShortTypeNames8100.typeparam(3) === Int
    @test ShortTypeNames8100.typeparam(2.0) === Float64
    @test ShortTypeNames8100.typeparam2(Int) === Int

    # Sanity: an actual free type variable value still reports as a TypeVar.
    @test typeof(TypeVar(:S)) === TypeVar
end

true
