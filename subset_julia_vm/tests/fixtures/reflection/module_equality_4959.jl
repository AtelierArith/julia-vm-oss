# Module equality compares module identity (Issue #4959)
# Upstream Julia: Base == Base => true, Base == Core => false
using Test

@testset "module equality" begin
    @test Base == Base
    @test Core == Core
    @test !(Base == Core)
    @test Base != Core
    @test Base === Base
    @test !(Base === Core)
    @test Base !== Core
end

true
