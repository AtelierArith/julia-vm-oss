using Test

@testset "Rational unsigned components use show form (Issue #9374)" begin
    @test sprint(show, 0x01 // 0x03) == "0x01//0x03"
    @test repr(0x01 // 0x03) == "0x01//0x03"
    @test string(0x01 // 0x03) == "0x01//0x03"
    @test sprint(show, true // true) == "true//true"
end

true
