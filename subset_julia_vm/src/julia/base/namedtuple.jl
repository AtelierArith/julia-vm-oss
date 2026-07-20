# =============================================================================
# namedtuple.jl - NamedTuple merge dispatch
# =============================================================================
# Based on Julia's base/namedtuple.jl
#
# Keyword-argument splatting (`f(; source...)`) must route through real
# `merge(::NamedTuple, source)` multiple dispatch so user-defined `Base.merge`
# extensions and Base specializations actually run, instead of being decided
# entirely inside the Rust runtime (Issue #11381). The VM's keyword-splat
# preparation (`Vm::merge_kwarg_splat_source`) calls this generic function by
# name — exactly like any other multi-method call — so it never special-cases
# a struct name in Rust (repository rule 8).
#
# Only the methods needed to make that dispatch structurally sound are ported
# here:
#
#   - The three degenerate two-`NamedTuple` merges that need no runtime
#     `NamedTuple` construction (`merge_names`/`merge_fallback` in upstream)
#     — they simply return one of the two inputs unchanged.
#   - `merge(a::NamedTuple, b::Zip{I1,I2})`, upstream's
#     `merge(a::NamedTuple, b::Iterators.Zip{<:Tuple{Any,Any}})`: validates
#     that `b`'s keys have no duplicates the same way upstream's
#     `NamedTuple{names}(...)` constructor does, before any merged result is
#     built.
#
# The fully general *runtime* two-non-empty-`NamedTuple` merge (upstream's
# generated `merge(a::NamedTuple{an}, b::NamedTuple{bn})`) and the fully
# generic `merge(a::NamedTuple, itr)` fallback both need a runtime-parametric
# `NamedTuple{names}(values)` constructor (`names` not known until run time),
# which SubsetJuliaVM does not yet support — see Issue #11494 (follow-up filed
# alongside this change). This is narrower than "general NamedTuple merge is
# unimplemented": a *call-site-static* merge of two literal/statically-typed
# NamedTuples (e.g. `merge((a=1,b=2), (b=3,c=4))`) already works today via the
# compiler's `try_compile_named_tuple_merge` constant-fold fast path
# (`subset_julia_vm_compile/src/compile/expr/call/mod.rs`) and is unaffected
# by this file — only a *dynamic* merge whose operand field-name sets are not
# statically known at the call site (e.g. inside a function generic over
# `::NamedTuple`, or a keyword-splat source that isn't one of the methods
# below) remains blocked on #11494. Once dispatch selects one of the methods
# below, the VM's own structural `iterate`-based keyword-splat accumulation
# (already used for every keyword source with no applicable `merge` method)
# finishes building the merged result — equivalent to upstream's
# `merge(a, NamedTuple{...}(...))` without needing that constructor. This
# means `merge(a::NamedTuple, b::Zip{I1,I2})` below is only a complete,
# upstream-equivalent `merge` when reached through keyword-splat preparation;
# called directly (`merge(nt, zip(...))`) it returns `b` unchanged rather than
# the merged NamedTuple upstream would (also tracked by #11494).

# Degenerate two-NamedTuple merges (Issue #11381): mirror upstream's
# special-cased empty-side methods in `base/namedtuple.jl` — no runtime
# NamedTuple construction needed since the result is always one of the inputs.
merge(a::NamedTuple, b::NamedTuple{()}) = a
merge(a::NamedTuple{()}, b::NamedTuple{()}) = a
merge(a::NamedTuple{()}, b::NamedTuple) = b

# `merge(a::NamedTuple, b::Iterators.Zip{<:Tuple{Any,Any}})` (Issue #11381):
# reject duplicate keys the same way upstream's `NamedTuple{names}(...)`
# constructor does before any merged result is built (upstream:
# `merge(a, NamedTuple{Tuple(b.is[1])}(b.is[2]))`, whose `NamedTuple{...}`
# construction raises this exact `ErrorException` on a duplicate name).
# Returns `b` unchanged once validated — see the file-level note above on why
# that is only a complete `merge` through the keyword-splat caller (#11494).
function merge(a::NamedTuple, b::Zip{I1,I2}) where {I1,I2}
    seen = Symbol[]
    for k in b.itr1
        ks = k::Symbol
        if ks in seen
            error("duplicate field name in NamedTuple: \"$(ks)\" is not unique")
        end
        push!(seen, ks)
    end
    b
end
