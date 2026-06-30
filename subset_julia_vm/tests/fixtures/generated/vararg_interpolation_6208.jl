using Test

# Issue #6208 / #5074: `$x` in a generated vararg body observes the generated
# type tuple, not the runtime argument value tuple.

@generated function generated_vararg_interpolation_6208(x...)
    :($x)
end
generated_vararg_interpolation_6208() = ()

@generated generated_vararg_interpolation_short_6208(x...) = :($x)
generated_vararg_interpolation_short_6208() = ()

@testset "generated vararg interpolation (Issue #6208)" begin
    @test generated_vararg_interpolation_6208() == ()
    @test generated_vararg_interpolation_6208(1) == (Int64,)
    @test generated_vararg_interpolation_6208(1, 2) == (Int64, Int64)
    @test generated_vararg_interpolation_6208(1, 2, 3) == (Int64, Int64, Int64)
    @test generated_vararg_interpolation_short_6208() == ()
    @test generated_vararg_interpolation_short_6208(1) == (Int64,)
    @test generated_vararg_interpolation_short_6208(1, 2) == (Int64, Int64)
    @test generated_vararg_interpolation_short_6208(1, 2, 3) == (Int64, Int64, Int64)
end

generated_vararg_interpolation_6208() == () &&
    generated_vararg_interpolation_6208(1) == (Int64,) &&
    generated_vararg_interpolation_6208(1, 2) == (Int64, Int64) &&
    generated_vararg_interpolation_6208(1, 2, 3) == (Int64, Int64, Int64) &&
    generated_vararg_interpolation_short_6208() == () &&
    generated_vararg_interpolation_short_6208(1) == (Int64,) &&
    generated_vararg_interpolation_short_6208(1, 2) == (Int64, Int64) &&
    generated_vararg_interpolation_short_6208(1, 2, 3) == (Int64, Int64, Int64)
