using Test

nameof_user_function_5580(x) = x + 1
nameof_user_function_any_5580(x::Any) = x + 1
nameof_user_function_zero_5580() = 1

@testset "nameof user-defined function (Issue #5580)" begin
    @test nameof(nameof_user_function_5580) == :nameof_user_function_5580
    @test nameof(nameof_user_function_any_5580) == :nameof_user_function_any_5580
    @test nameof(nameof_user_function_zero_5580) == :nameof_user_function_zero_5580
    @test nameof(sin) == :sin

    @test nameof_user_function_5580(2) == 3
    @test nameof_user_function_any_5580(2) == 3
    @test nameof_user_function_zero_5580() == 1
end

true
