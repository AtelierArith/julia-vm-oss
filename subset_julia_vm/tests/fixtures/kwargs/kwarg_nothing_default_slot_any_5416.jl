using Test

function kw_default_identity_5416(x; by=nothing)
    return by
end

@testset "kwarg nothing default remains dynamic at slot boundary (Issue #5416)" begin
    @test kw_default_identity_5416(1) === nothing
    @test kw_default_identity_5416(1; by=10) == 10
    @test kw_default_identity_5416(1; by=true) === true
end

true
