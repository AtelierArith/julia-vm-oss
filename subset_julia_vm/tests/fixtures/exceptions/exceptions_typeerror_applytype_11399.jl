# `x{T}` where x is a VALUE (not a type) raises a TypeError whose typed
# payload matches upstream — func = Symbol("Type{...} expression"),
# expected = UnionAll, got = the value itself — instead of the
# :unknown/nothing placeholders the VM funnel used to emit (Issue #11399,
# same payload-restoration line as #11374/DomainError). typeassert and other
# TypeError paths are unaffected.
using Test

@testset "apply-type-to-value TypeError payload (Issue #11399)" begin
    x = 5
    e = try
        x{Int}
    catch err
        err
    end
    @test e isa TypeError
    @test e.func == Symbol("Type{...} expression")
    @test e.expected == UnionAll
    @test e.got === 5

    # got is the actual value, of any type.
    y = "s"
    e2 = try
        y{Float64}
    catch err
        err
    end
    @test e2 isa TypeError
    @test e2.func == Symbol("Type{...} expression")
    @test e2.got === "s"

    nested = try
        x{Int}
    catch
        try
            y{Float64}
        catch
        end
        try
            rethrow()
            nothing
        catch rethrown
            rethrown
        end
    end
    @test nested isa TypeError
    @test nested.func == Symbol("Type{...} expression")
    @test nested.expected == UnionAll
    @test nested.got === 5
end

@testset "other TypeError paths keep their payloads (Issue #11399)" begin
    # typeassert carries its own real fields (pure-Julia struct throw).
    e = try
        (1)::String
    catch err
        err
    end
    @test e isa TypeError
    @test e.func === :typeassert
    @test e.expected == String
    @test e.got === 1
end

true
