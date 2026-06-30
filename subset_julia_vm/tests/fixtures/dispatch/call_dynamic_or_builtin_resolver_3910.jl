using Test

function floor_dynamic_or_builtin_via_any_3910(x)
    y::Any = x
    floor(y)
end

function ceil_dynamic_or_builtin_via_any_3910(x)
    y::Any = x
    ceil(y)
end

@testset "CallDynamicOrBuiltin shared resolver (Issue #3910)" begin
    r = 7 // 3
    @test floor_dynamic_or_builtin_via_any_3910(r) == 2.0
    @test ceil_dynamic_or_builtin_via_any_3910(r) == 3.0

    @test floor_dynamic_or_builtin_via_any_3910(3.7) == 3.0
    @test ceil_dynamic_or_builtin_via_any_3910(3.2) == 4.0
end

true
