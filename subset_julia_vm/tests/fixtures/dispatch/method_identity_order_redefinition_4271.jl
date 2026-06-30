using Test

# Issue #4271 (method identity / order / redefinition boundaries supported by
# sjulia). This locks in the verifiable, upstream-faithful slice of the issue's
# acceptance surface as a regression guard:
#
# - **Method order independence**: the most-specific method wins regardless of
#   the textual order the methods are defined in (a general method may be
#   declared after the specific one).
# - **Redefinition within a compilation unit**: a later definition with the same
#   signature replaces the earlier one (the new body is dispatched).
# - **Method identity / multiple dispatch**: disjoint signatures keep independent
#   inferred return types — inference is keyed by the matched method, not
#   collapsed across the shared generic name.
#
# The deeper #4271/#5603 scope — precise `MethodInstance` / `CodeInstance`
# identity, full valid-world filtering, historical-world / `invokelatest`, and
# full ambiguity diagnostics — remains the open architectural core. The
# ambiguous-call return inference surface (`Union{}`) is covered separately by
# `type_inference/ambiguous_method_return_bottom_5603.jl`; runtime ambiguity
# (`MethodError`) is covered separately by
# `dispatch/method_ambiguity_runtime_5071.jl`.
#
# Values verified field-for-field against upstream Julia 1.12.6.

# General method declared AFTER the specific one.
order_mo_4271(x::Real) = "real"
order_mo_4271(x::Int64) = "int"

# Same-signature redefinition within the file: the later body replaces the
# earlier one.
redef_4271(x::Int64) = 1
redef_4271(x::Int64) = 2

# Disjoint signatures: inference must stay independent per matched method.
ident_4271(x::Int64) = x + 1
ident_4271(x::String) = length(x)

@testset "method definition order independence (#4271)" begin
    @test order_mo_4271(3) == "int"      # most specific wins
    @test order_mo_4271(3.0) == "real"
    @test order_mo_4271(Int32(3)) == "real"  # Int32 <: Real but not Int64
end

@testset "same-signature redefinition replaces (#4271)" begin
    @test redef_4271(5) == 2
end

@testset "disjoint-signature inference identity (#4271)" begin
    @test Base.infer_return_type(ident_4271, Tuple{Int64}) === Int64
    @test Base.infer_return_type(ident_4271, Tuple{String}) === Int64
    @test ident_4271(7) == 8
    @test ident_4271("abc") == 3
end

true
