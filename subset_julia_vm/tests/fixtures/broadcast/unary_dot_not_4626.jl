using Test

@testset "prefix dotted not broadcast (#4626, #4640)" begin
    xs = Bool[true, false, true]
    ys = .!xs

    @test eltype(ys) === Bool
    @test length(ys) == 3
    @test ys[1] === false
    @test ys[2] === true
    @test ys[3] === false

    zs = .!Bool[false, true]
    @test length(zs) == 2
    @test zs[1] === true
    @test zs[2] === false

    @test (!)(false) === true
    @test typeof((!)) === typeof(!)

    ws = (!).(Bool[true, false])
    @test eltype(ws) === Bool
    @test length(ws) == 2
    @test ws[1] === false
    @test ws[2] === true
end

true
