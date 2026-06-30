using Test

# Issue #4274: minimal Base.infer_effects / Base.infer_exception_type surface.
# Simple pure user methods infer to the TOTAL effects representative with an
# empty (Union{}) exception type in upstream Julia 1.12.

reflection_effects_add_4274(x, y) = x + y
reflection_effects_sq_4274(x) = x * x
reflection_effects_ident_4274(x) = x

@testset "reflection infer_effects and infer_exception_type basic" begin
    # infer_exception_type for simple total functions is Union{}.
    @test Base.infer_exception_type(reflection_effects_add_4274, Tuple{Int64,Int64}) === Union{}
    @test Base.infer_exception_type(reflection_effects_sq_4274, Tuple{Int64}) === Union{}
    @test Base.infer_exception_type(reflection_effects_ident_4274, Tuple{Float64}) === Union{}

    # infer_effects returns an Effects object whose accessor fields match upstream.
    ef = Base.infer_effects(reflection_effects_add_4274, Tuple{Int64,Int64})

    # UInt8 bitfields default to ALWAYS_TRUE (0x00) for proven-total methods.
    @test ef.consistent === 0x00
    @test ef.effect_free === 0x00
    @test ef.inaccessiblememonly === 0x00
    @test ef.noub === 0x00
    @test ef.nonoverlayed === 0x00

    # Bool fields are true for proven-total methods.
    @test ef.nothrow === true
    @test ef.terminates === true
    @test ef.notaskstate === true
    @test ef.nortcall === true

    # Custom show matches the upstream Effects key format exactly.
    @test string(ef) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"

    # Field names match upstream Compiler.Effects in order.
    @test fieldnames(typeof(ef)) === (:consistent, :effect_free, :nothrow, :terminates,
        :notaskstate, :inaccessiblememonly, :noub, :nonoverlayed, :nortcall)

    # Single-argument forms reflect over all methods.
    @test Base.infer_exception_type(reflection_effects_ident_4274) === Union{}
    @test string(Base.infer_effects(reflection_effects_ident_4274)) == "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end

true
