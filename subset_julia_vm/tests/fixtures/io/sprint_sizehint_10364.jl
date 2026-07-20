# sprint(f, args...; sizehint=N) accepts the sizehint keyword (Issue #10364)
#
# Upstream `sprint(f::Function, args...; context=nothing, sizehint::Integer=0)`
# treats sizehint as a preallocation hint with no effect on the returned
# string. sjulia's compile_sprint used to drop every kwarg except `context`
# before the fast builtin path, so a context-free `sizehint` call raised an
# unsupported-keyword MethodError.

using Test

@testset "sprint sizehint keyword" begin
    @test sprint(print, "abc"; sizehint=10) == "abc"
    @test sprint(show, 42; sizehint=64) == "42"
    @test sprint(print, 1, 2, 3; sizehint=0) == "123"
    # sizehint composes with context (compact-show precision divergence for
    # longer floats and the non-Float64 context path are pre-existing,
    # separate gaps — see Issue #11419)
    @test sprint(show, 3.14; context=:compact => true, sizehint=32) == "3.14"
end

true
