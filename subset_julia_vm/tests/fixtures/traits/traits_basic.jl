using Test

@testset "OrderStyle traits" begin
    @test isa(Ordered(), OrderStyle)
    @test isa(Unordered(), OrderStyle)
end

@testset "ArithmeticStyle traits" begin
    @test isa(ArithmeticRounds(), ArithmeticStyle)
    @test isa(ArithmeticWraps(), ArithmeticStyle)
    @test isa(ArithmeticUnknown(), ArithmeticStyle)
end

@testset "RangeStepStyle traits" begin
    @test isa(RangeStepRegular(), RangeStepStyle)
    @test isa(RangeStepIrregular(), RangeStepStyle)
end

true
