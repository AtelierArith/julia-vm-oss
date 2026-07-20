using Test

# Issue #8441: the fixed effect name hints for `isless` (and the call form of
# `!==`) were retired from the Rust `infer_builtin_effects` table. Their
# summaries are now proven from the Base method bodies by the body-derived
# effect walker, so wrappers must still infer the upstream TOTAL record —
# both when the retired name is the direct call and when it sits nested
# inside another expression (ternary condition).
#
# Upstream Julia parity (1.12): every wrapper below infers
# `(+c,+e,+n,+t,+s,+m,+u,+o,+r)` and an empty (`Union{}`) exception type.

const RETIRED_CMP_TOTAL_8441 = "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

retired_isless_wrap_8441(a, b) = isless(a, b)
nested_isless_ternary_8441(a, b) = isless(a, b) ? a : b

@testset "retired comparison effect hints stay body-provable (Issue #8441)" begin
    @test string(Base.infer_effects(retired_isless_wrap_8441, Tuple{Int64,Int64})) ==
          RETIRED_CMP_TOTAL_8441
    @test string(Base.infer_effects(nested_isless_ternary_8441, Tuple{Int64,Int64})) ==
          RETIRED_CMP_TOTAL_8441
    @test Base.infer_exception_type(retired_isless_wrap_8441, Tuple{Int64,Int64}) === Union{}
end

true
