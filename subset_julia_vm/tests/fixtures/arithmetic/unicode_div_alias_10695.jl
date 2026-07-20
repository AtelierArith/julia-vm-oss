# ÷ resolves as a first-class function binding (Issue #10695)
#
# `x ÷ y` in direct code lowers straight to div, but macro-expanded forms
# (@show 7 ÷ 2) and value uses (f = ÷) re-dispatch the OPERATOR NAME, which
# had no methods. Upstream aliases `const ÷ = div`; sjulia mirrors it with a
# forwarding method.

using Test

@testset "unicode ÷ as a name" begin
    @test (÷)(7, 2) == 3
    f = ÷
    @test f(9, 4) == 2
    @test ismissing((÷)(missing, 1))
end

@testset "@show with ÷" begin
    io = IOBuffer()
    a = Any[missing, 1]
    r = a[1] ÷ a[2]
    @test ismissing(r)
    # the issue MWE: @show must not raise
    @show a[1] ÷ a[2]
    @test true
end

true
