using Test

@testset "essentials core primitives phase 1" begin
    @test typeassert(1, Int64) == 1
    @test typeassert("x", String) == "x"

    @test Base.cconvert(Int64, 3.0) == 3
    @test Base.cconvert(Float64, 3) == 3.0
    @test Base.unsafe_convert(Int64, 3) == 3
    @test Base.unsafe_convert(String, "abc") == "abc"

    @test Base.unwrap_unionall(Int64) === Int64
    @test Base.isvarargtype(Int64) == false
    @test Base.isvatuple(Tuple) == true
    @test Base.donotdelete(42) === nothing
    @test Base.compilerbarrier(:type, "value") == "value"
end

true
