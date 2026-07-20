using Test

# Issue #8441: the fixed effect name hints for `ifelse` and `tuple` were
# retired from the Rust `infer_builtin_effects` table. Both are single pure
# Julia Base methods (base/essentials.jl, base/tuple.jl) whose bodies prove
# the same total summary through the body-derived effect walker, so wrappers
# must still infer the upstream TOTAL record.
#
# Upstream Julia parity (1.12): every wrapper below infers
# `(+c,+e,+n,+t,+s,+m,+u,+o,+r)` and an empty (`Union{}`) exception type.

const RETIRED_CORE_TOTAL_8441 = "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

retired_ifelse_wrap_8441(c, x, y) = ifelse(c, x, y)
retired_tuple_wrap_8441(a, b) = tuple(a, b)

@testset "retired core effect hints stay body-provable (Issue #8441)" begin
    @test string(Base.infer_effects(retired_ifelse_wrap_8441, Tuple{Bool,Int64,Int64})) ==
          RETIRED_CORE_TOTAL_8441
    @test string(Base.infer_effects(retired_tuple_wrap_8441, Tuple{Int64,Int64})) ==
          RETIRED_CORE_TOTAL_8441
    @test Base.infer_exception_type(retired_ifelse_wrap_8441, Tuple{Bool,Int64,Int64}) ===
          Union{}
end

true
