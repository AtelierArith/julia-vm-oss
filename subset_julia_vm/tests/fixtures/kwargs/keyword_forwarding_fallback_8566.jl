using Test

kwforward8566(x::Int) = "int"
kwforward8566(xs...; kws...) = "fallback"

kwdecl8566(x::Int; opt = 0) = "intkw"
kwdecl8566(xs...; kws...) = "fallback"

kwnumber8566(x::Int) = "int"
kwnumber8566(x::Number; opt = 0) = "numberkw"
kwnumber8566(xs...; kws...) = "fallback"

kwmissing8566(x::Int) = "int"

module KWForwardingFallback8566
export q

q(x::Int) = "int"
q(xs...; kws...) = "fallback"

end

@testset "keyword forwarding fallback dispatch (Issue #8566)" begin
    @test kwforward8566(1) == "int"
    @test kwforward8566(1; opt = 1) == "fallback"
    @test kwforward8566(1.5; opt = 1) == "fallback"
    @test kwdecl8566(1; opt = 1) == "intkw"
    @test kwnumber8566(1; opt = 2) == "numberkw"
    @test KWForwardingFallback8566.q(1; opt = 1) == "fallback"
    @test_throws MethodError kwmissing8566(1; opt = 1)
end

true
