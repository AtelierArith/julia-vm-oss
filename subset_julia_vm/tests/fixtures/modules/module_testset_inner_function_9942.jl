# A function defined inside a `@testset` inside a `module` must resolve when
# called within that same testset (Issue #9942). `Test.@testset` macro-expands
# to a `let ... _testset_begin!(...) ... _testset_end!() end`, so the testset
# body is a marked LetBlock in the module body; the module-body inline-function
# collector must descend into it to register the helper.
module M9942
using Test

@testset "inner named function resolves" begin
    f(x) = x + 1
    @test f(1) == 2
    @test f(10) == 11
end

@testset "second testset, distinct helper" begin
    g(x) = x * 3
    @test g(2) == 6
    @test g(0) == 0
end
end

true
