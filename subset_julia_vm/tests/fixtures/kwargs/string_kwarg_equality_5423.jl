using Test

function kw_string_identity_5423(x; by=nothing)
    return by
end

@testset "String kwarg values compare by contents after dynamic return (Issue #5423)" begin
    @test kw_string_identity_5423(1; by="x") == "x"
    @test kw_string_identity_5423(1; by="x") != "y"
    @test kw_string_identity_5423(1; by="a") < "b"
end

true
