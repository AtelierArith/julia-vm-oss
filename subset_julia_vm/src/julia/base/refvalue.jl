# =============================================================================
# RefValue / Ref wrappers
# =============================================================================
# Based on Julia's base/refvalue.jl and base/refpointer.jl.
#
# SubsetJuliaVM still stores RefValue cells in the VM value model so broadcast,
# mutation, dispatch, and display keep their existing behavior. The public Base
# names are ordinary Julia methods; they call underscored VM boundaries.

Ref(x) = _ref_new(x)

getindex(x::Ref) = _ref_get(x)
