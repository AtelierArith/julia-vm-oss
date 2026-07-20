# A type alias whose RHS is another bare type alias (a "chain") must be
# preserved as a static alias resolving to the ORIGINAL canonical target, no
# matter how many alias links separate the annotation from that target
# (Issue #11099).
#
# Root cause: `lowering::stmt::is_type_expression` accepted a bare uppercase
# RHS identifier as a type-alias target only from a fixed builtin-name list,
# so `const B = A` was not itself extracted as a `TypeAliasDef` when `A` was
# only a user-registered alias (not a builtin, not yet a declared struct/etc).
# The runtime binding for `B` still existed, but a signature `f(x::B)` lowered
# without the canonical target and could never dispatch (`MethodError`).
#
# Fix (landed as part of Issue #11104): the alias-classification gate
# (`is_likely_type_name`) also consults `type_alias::is_registered_alias`, and
# the pre-scan (`prescan_and_register_type_aliases`) iterates the binding walk
# to a fixpoint so a chain resolves regardless of how many links it has or
# what order the `const` statements appear in within the pre-scan. This
# fixture pins 2- and 3-link chains for both a builtin target and a
# user-declared struct target, closing the gap #11099 tracked explicitly.
#
# All expectations verified against upstream Julia 1.12.

using Test

# --- 2-link chain, builtin target (the exact Issue #11099 MWE) ---
const A11099 = Int64
const B11099 = A11099
f_builtin_2link_11099(x::B11099) = x + 1

# --- 3-link chain, builtin target ---
const C11099 = B11099
f_builtin_3link_11099(x::C11099) = x + 2

# --- 2-link chain, struct target ---
struct S11099
    v::Int
end
const SA11099 = S11099
f_struct_2link_11099(x::SA11099) = x.v + 10

# --- 3-link chain, struct target ---
const SB11099 = SA11099
f_struct_3link_11099(x::SB11099) = x.v + 20

# --- 3-link chain, parametric alias target (Vector{T} through a plain alias
# chain, keeping the bare-name classification path exercised for a
# non-builtin, non-struct RHS too) ---
const VecAlias11099 = Vector{Int64}
const VecAlias2_11099 = VecAlias11099
const VecAlias3_11099 = VecAlias2_11099
f_vec_3link_11099(x::VecAlias3_11099) = length(x)

@testset "2-link alias chain dispatches to the canonical target (Issue #11099)" begin
    @test f_builtin_2link_11099(41) == 42
    @test f_struct_2link_11099(S11099(5)) == 15
end

@testset "3-link alias chain dispatches to the canonical target (Issue #11099)" begin
    @test f_builtin_3link_11099(40) == 42
    @test f_struct_3link_11099(S11099(1)) == 21
    @test f_vec_3link_11099([1, 2, 3]) == 3
end

@testset "chained aliases still resolve as values (Issue #11099)" begin
    @test B11099 === Int64
    @test C11099 === Int64
    @test SA11099 === S11099
    @test SB11099 === S11099
end

true
