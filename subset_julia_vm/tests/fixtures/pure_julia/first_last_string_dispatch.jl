# Test first/last on String goes through Pure Julia method dispatch (Issue #3734).
# Pure Julia methods live in base/strings/basic.jl.

using Test

@testset "first/last on String dispatch through Pure Julia (Issue #3734)" begin
    @test (first("abc") == 'a')
    @test (last("abc") == 'c')
    @test (first("z") == 'z')
    @test (last("z") == 'z')
end

true
