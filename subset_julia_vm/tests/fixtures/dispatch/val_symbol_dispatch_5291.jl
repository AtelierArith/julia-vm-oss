# Val{:sym} multiple dispatch must route by the symbol value parameter (Issue #5291)
#
# f(::Val{:up}) / f(::Val{:down}) previously always dispatched to the first method
# because the runtime type of Val(:up) rendered as Val{Symbol("up")} while the
# method parameter Val{:up} used the colon spelling, so isa/dispatch never matched.

using Test

f(::Val{:up}) = "went up"
f(::Val{:down}) = "went down"

@testset "Val{:sym} multiple dispatch (Issue #5291)" begin
    @test f(Val(:up)) == "went up"
    @test f(Val(:down)) == "went down"

    # isa against a symbol value parameter
    @test Val(:up) isa Val{:up}
    @test !(Val(:up) isa Val{:down})

    # typeof renders an identifier-symbol parameter in colon form (matching upstream)
    @test string(typeof(Val(:up))) == "Val{:up}"
    @test string(typeof(Val(:func!))) == "Val{:func!}"

    # a non-identifier symbol keeps the Symbol("...") spelling
    @test string(typeof(Val(Symbol("a b")))) == "Val{Symbol(\"a b\")}"
end

true
