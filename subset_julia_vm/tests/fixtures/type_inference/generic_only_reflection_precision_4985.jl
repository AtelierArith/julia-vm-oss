using Test

# Issue #4985: body-based reflection precision for a generic-only method.
#
# A generic single-method function whose body returns a literal must report
# the literal's concrete type from `Base.infer_return_type` / `Base.return_types`,
# both with an explicit `Tuple` signature and with no signature (all methods).
# Upstream Julia reports the concrete literal type here rather than a
# conservative widen.
#
# This fixture locks in the GENERIC-ONLY state only. The separate world-age
# portion of #4985 (reflection seeing a later, more-specific top-level method
# before top-level execution reaches that definition) is intentionally not
# exercised here; it remains tracked under #4985 / the #4271 world-age epic.

reflection_generic_only_int_4985(x) = 1
reflection_generic_only_float_4985(x) = 1.0
reflection_generic_only_str_4985(x) = "hi"
reflection_generic_only_id_4985(x) = x

@testset "generic-only method reflection precision (Issue #4985)" begin
    @test Base.infer_return_type(reflection_generic_only_int_4985, Tuple{Int64}) === Int64
    @test Base.return_types(reflection_generic_only_int_4985, Tuple{Int64}) == [Int64]
    @test Base.infer_return_type(reflection_generic_only_int_4985) === Int64
    @test Base.return_types(reflection_generic_only_int_4985) == [Int64]

    @test Base.infer_return_type(reflection_generic_only_float_4985) === Float64
    @test Base.infer_return_type(reflection_generic_only_str_4985) === String

    @test Base.infer_return_type(reflection_generic_only_id_4985, Tuple{Int64}) === Int64
end

true
