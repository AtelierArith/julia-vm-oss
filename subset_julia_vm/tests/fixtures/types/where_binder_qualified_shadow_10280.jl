# where-binder shadowing only the BARE name, never a module-qualified
# reference (Issue #10280, tech-debt epic #10049): upstream Julia's lexical
# `where`-binder scoping shadows only the unqualified spelling of a binder's
# name. An explicitly module-qualified reference whose LAST component happens
# to equal the binder name (`Core.Builtin` under a `where Builtin`) is NOT
# shadowed, so the binder is unused in the body and the `where` collapses to
# the concrete `Vector{Core.Builtin}` DataType.
#
# Previously sjulia's keep-vs-drop check (`julia_type_references_typevar` /
# `type_name_references_typevar`, subset_julia_vm_vm/src/vm/builtins_types.rs)
# tokenized `Core.Builtin` into ["Core", "Builtin"] and matched the bare token
# `Builtin`, so it wrongly reported the body as referencing the binder and kept
# a spurious `UnionAll`. The fix skips a token that is module-qualified
# (immediately preceded by `.`); it is general over any qualified path
# (`M.N.Name`), not a `Core.`-name special case.
#
# All expectations below were verified against upstream Julia 1.12.

using Test

@testset "qualified reference is not shadowed by a bare binder (Issue #10280)" begin
    # Reported MWE: binder `Builtin` matches the LAST component of the
    # module-qualified `Core.Builtin`. Upstream leaves `Core.Builtin` concrete,
    # the binder is unused, and the `where` is dropped.
    r = Vector{Core.Builtin} where Builtin<:Function
    @test typeof(r) == DataType
    @test !(r isa UnionAll)
    @test r == Vector{Core.Builtin}
    @test string(r) == "Vector{Core.Builtin}"
    @test Vector{Core.Builtin} <: r
end

@testset "user/Base-qualified path last component is not shadowed (Issue #10280)" begin
    # Generalizes beyond `Core.`: a binder spelled like the last component of a
    # nested Base-qualified concrete type is likewise not shadowed.
    r2 = Vector{Base.RefValue{Int64}} where RefValue<:Any
    @test typeof(r2) == DataType
    @test r2 == Vector{Base.RefValue{Int64}}
end

@testset "regression guards: a BARE binder occurrence still shadows (Issue #10280)" begin
    # An ordinary (non-colliding) binder used bare in the body keeps its
    # `where` -- the qualified-skip must not swallow bare references.
    a = Vector{T} where T<:Function
    @test typeof(a) == UnionAll

    # A bare binder whose spelling collides with a builtin type name still
    # shadows it lexically (Issue #10100 behavior must be preserved: the `where`
    # is kept and the shadowed occurrence sees through to the real bound).
    b = Vector{Int64} where Int64<:Real
    @test typeof(b) == UnionAll
    @test Float64[1.0] isa b
end

true
