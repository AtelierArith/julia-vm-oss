# Issue #7901: single-arg rand(x)/randn(x) where `x` is statically `Any`
# (function return, struct field, Any[] element, ::Any parameter) must defer to
# the user/library `rand`/`randn` method for that struct type instead of being
# treated as a dimension by the Rust builtin (which errors on a StructRef).
using Test
struct Picker
    probs::Vector{Float64}
end
import Base: rand, randn
rand(d::Picker) = argmax(d.probs)
randn(d::Picker) = -argmax(d.probs)

f(x::Any) = rand(x)
g(x::Any) = randn(x)

struct Holder
    p::Picker
end
h = Holder(Picker([0.0, 0.0, 1.0]))

# concrete arg: dispatch resolves the user method at compile time.
@test rand(Picker([0.0, 1.0])) == 2
# ::Any parameter: routed through the Rust builtin, must defer to user rand.
@test f(Picker([0.0, 1.0])) == 2
# ::Any parameter for randn.
@test g(Picker([0.0, 1.0])) == -2
# Any[] element is statically Any.
@test rand(Any[Picker([0.0, 1.0, 0.0])][1]) == 2
# struct field read is statically Any here too.
@test rand(h.p) == 3

# A plain integer dimension argument still produces a vector (no regression).
dim(n::Any) = rand(n)
@test length(dim(3)) == 3

println("all 7901 checks passed")
true
