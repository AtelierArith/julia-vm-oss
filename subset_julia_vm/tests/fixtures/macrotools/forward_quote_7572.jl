using MacroTools
using Test

@testset "MacroTools @forward quoted generator method definitions (Issue #7572)" begin
    fs = [:alpha_7572, :beta_7572]
    T = :ForwardTarget7572
    field = :payload

    ex = :($([:($f(x::$T, args...; kwargs...) =
               (Base.@_inline_meta; $f(x.$field, args...; kwargs...)))
             for f in fs]...);
          nothing)

    @test ex isa Expr
    @test ex.head == :block
    @test length(ex.args) >= 3
    text = string(ex)
    @test contains(text, "alpha_7572")
    @test contains(text, "beta_7572")
end

true
