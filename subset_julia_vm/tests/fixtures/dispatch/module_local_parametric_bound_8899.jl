# Issue #8899: a module-local parametric struct whose type parameter is bounded
# by a module-local abstract type must construct without a spurious compile-time
# bound-violation. The struct's `parent_type` is stored module-qualified
# (`M.Ring`) but the parametric type-parameter bound (`R <: Ring`) was stored
# bare, so the compile-time check compared `"M.Ring" == "Ring"` and wrongly
# rejected `M.Poly{M.BaseRing}(...)`. Runtime `<:` and the top-level (non-module)
# form were always correct; this guards the module-local compile-time path.
module M8899
abstract type Ring end
struct BaseRing <: Ring end
struct Poly{R <: Ring}
    base::R
end
end

p = M8899.Poly{M8899.BaseRing}(M8899.BaseRing())

ok = (p.base isa M8899.BaseRing) &&
     (M8899.BaseRing <: M8899.Ring) &&
     (typeof(p) === M8899.Poly{M8899.BaseRing})

println(ok)
ok
