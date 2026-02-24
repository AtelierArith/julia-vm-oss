using Test

@testset "Traits subtypes" begin
    @test Ordered <: OrderStyle
    @test Unordered <: OrderStyle
    @test ArithmeticRounds <: ArithmeticStyle
    @test ArithmeticWraps <: ArithmeticStyle
    @test ArithmeticUnknown <: ArithmeticStyle
    @test RangeStepRegular <: RangeStepStyle
    @test RangeStepIrregular <: RangeStepStyle
end

@testset "Traits equality" begin
    @test Ordered() == Ordered()
    @test Unordered() == Unordered()
    @test Ordered() != Unordered()
end

@testset "Traits typeof" begin
    @test typeof(Ordered()) == Ordered
    @test typeof(ArithmeticWraps()) == ArithmeticWraps
    @test typeof(RangeStepRegular()) == RangeStepRegular
end

true
