# A `const` alias of a BASE/STDLIB-DECLARED type used as a PARAMETER
# ANNOTATION must dispatch (Issue #11113, sibling of #11104).
#
# `const MyPair = Pair; f(x::MyPair) = 1` registered the method against the
# nominal placeholder `Struct("MyPair")` — a type no value is ever an
# instance of — so `f(Pair(1, 2))` raised `MethodError`, even though the same
# alias worked fine as a value/constructor. Root cause: the lowering alias
# gate (`is_likely_type_name`) only recognizes type names the CURRENT program
# DECLARES (Issue #11104) plus a fixed builtin-name list; `Pair` is neither —
# it is declared inside Base, which is lowered in an isolated pass (or, under
# the Base bytecode cache, never lowered from source at all), so its name
# never reaches that gate and `MyPair` registered no alias at all.
#
# Fix: resolve any bare-identifier const/global binding still unregistered
# after lowering's own alias pass through the COMPILE-time tables that DO know
# every Base/stdlib type in both cache modes — `struct_table` for Pure-Julia
# struct declarations (`Pair`, `VersionNumber`, ...) and the compiler-visible
# builtin-type registry for native VM type names with no `struct_table` entry
# (`Regex`, `UnitRange`, ...). Neither table depends on a maintained name list,
# so this covers every Base/stdlib type structurally instead of one at a time.
#
# All expectations verified against upstream Julia 1.12.

using Test

const MyPair11113 = Pair
const MyRegex11113 = Regex
const MyRange11113 = UnitRange
const MyVersion11113 = VersionNumber

f_pair(x::MyPair11113) = 1
f_regex(x::MyRegex11113) = 2
f_range(x::MyRange11113) = 3
f_version(x::MyVersion11113) = 4

# Parametric use of a Base-struct alias (Issue #11113 follow-up scope).
f_pair_parametric(x::MyPair11113{Int64,Int64}) = 5

@testset "const alias of a Base/stdlib struct dispatches as a parameter annotation (Issue #11113)" begin
    @test f_pair(Pair(1, 2)) == 1
    @test f_regex(Regex("a+")) == 2
    @test f_range(1:5) == 3
    @test f_version(VersionNumber(1, 2, 3)) == 4
end

@testset "parametric use of a Base-struct alias dispatches (Issue #11113)" begin
    @test f_pair_parametric(Pair(1, 2)) == 5
end

@testset "the alias still works as a value / constructor (Issue #11113)" begin
    @test MyPair11113 === Pair
    @test MyPair11113(3, 4) isa Pair
    @test MyRegex11113("b+") isa Regex
    @test isa(Pair(5, 6), MyPair11113)
end

# Controls from #11104, re-checked alongside the new Base-struct case so a
# future regression cannot silently narrow the fix to only one alias family.
const MyInt11113 = Int64
struct E11113 end
const AE11113 = E11113

g_builtin(x::MyInt11113) = 6
g_struct(x::AE11113) = 7

@testset "builtin and program-declared alias controls still dispatch (Issue #11104)" begin
    @test g_builtin(3) == 6
    @test g_struct(E11113()) == 7
end

module Mod11113
struct Inner11113 end
const AInner11113 = Inner11113
inner_id(x::AInner11113) = 42
const AModPair11113 = Pair
pair_id(x::AModPair11113) = 43
end

@testset "module-local alias of a program-declared type still dispatches (Issue #11104)" begin
    @test Mod11113.inner_id(Mod11113.Inner11113()) == 42
end

@testset "module-local alias of a Base struct still dispatches (Issue #11113)" begin
    @test Mod11113.pair_id(Pair(1, 2)) == 43
end

true
