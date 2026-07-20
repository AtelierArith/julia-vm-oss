# Issue #11076 (dispatch-matching sibling of Issue #11021's type-identity
# fix; both discovered while investigating Issue #10989, StructId Phase 2b):
# two sibling modules declaring a same-named GENERIC struct, each used BARE
# (no explicit `{...}`) as a method parameter type, must not make dispatch
# wrongly ambiguous -- the caller's argument unambiguously matches exactly
# one of the two declared parameter types upstream.
#
# Root cause: module-prefix stripping in the runtime dispatch matcher
# (`Vm::type_matches`'s general `JuliaType::Struct` arm, `vm/dispatch.rs`)
# was unconditional, exactly like the #11021 type-equality bug -- fixed with
# the SAME asymmetric rule
# (`subset_julia_vm_types::types::struct_owners_compatible`, made `pub` and
# reused here rather than re-derived): a BARE reference legitimately denotes
# the same type as a QUALIFIED reference to it (Issue #8100), but two
# qualified references with DIFFERENT owners must stay distinct.
#
# Scope note: this fixture covers exactly the shape that resolves through
# runtime dispatch and is fixed here -- a GENERIC struct (`struct Box{T}
# ... end`) referenced BARE (no explicit `{...}`) in the method parameter
# annotation. Other shapes (a non-generic sibling-module struct, an
# EXPLICITLY parametric annotation such as `x::A1x.Box{Int}`, or a bare
# annotation naming a struct spelled like a builtin container family) hit a
# DIFFERENT, deeper bug: the method table silently collapses the second
# same-family method at REGISTRATION time
# (`subset_julia_vm_bytecode::method_table::MethodTable::add_method`'s
# `core_signature`-based redefinition dedup, which -- like the dispatch
# matcher before this fix -- loses module qualification because
# `CoreType::Struct`'s `name` field is always module-stripped), before any
# dispatch even runs. That registration-time bug is tracked separately as
# Issue #11094 and deferred to the Issue #11078 CoreType-module-awareness
# continuation; it is NOT fixed by this change.
using Test

module A1x11076
struct Box{T}
    x::T
end
end

module A2x11076
struct Box{T}
    x::T
end
end

f11076(x::A1x11076.Box) = "from A1x11076"
f11076(x::A2x11076.Box) = "from A2x11076"

# Nested modules: same base struct name at the same nesting depth under
# different parents must still resolve to the sibling matching the actual
# argument's owner, not collide into false ambiguity.
module Outer1x11076
module Inner1x11076
struct Box{T}
    x::T
end
end
end

module Outer2x11076
module Inner2x11076
struct Box{T}
    x::T
end
end
end

g11076(x::Outer1x11076.Inner1x11076.Box) = "nested1"
g11076(x::Outer2x11076.Inner2x11076.Box) = "nested2"

# Sanity: a struct that exists in only ONE module still dispatches normally
# (no sibling to be falsely ambiguous against -- the owner guard must not
# reject a legitimate unique match).
module OnlyMod11076
struct Box{T}
    x::T
end
end

h11076(x::OnlyMod11076.Box) = "OnlyMod11076"

# Control: a genuine (non-module) ambiguity must still be reported as
# ambiguous -- the owner-identity fix must not paper over real ambiguity
# unrelated to module qualification.
ambig11076(x::Int, y) = 1
ambig11076(x, y::Int) = 2

@testset "sibling-module struct param dispatch owner identity (Issue #11076)" begin
    @test f11076(A1x11076.Box(1)) == "from A1x11076"
    @test f11076(A2x11076.Box(2)) == "from A2x11076"

    @test g11076(Outer1x11076.Inner1x11076.Box(1)) == "nested1"
    @test g11076(Outer2x11076.Inner2x11076.Box(2)) == "nested2"

    @test h11076(OnlyMod11076.Box(1)) == "OnlyMod11076"

    @test_throws MethodError ambig11076(1, 1)
end

true
