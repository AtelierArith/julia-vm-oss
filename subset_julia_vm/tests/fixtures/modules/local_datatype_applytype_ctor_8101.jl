using Test

# Issue #8101: applying explicit type parameters to the `DataType` VALUE of a
# parametric struct held in a local variable (`t = A.Pt; t{Float64}(1.0, 2.0)`)
# must construct `A.Pt{Float64}` — honouring the EXPLICIT `{Float64}` and
# converting the arguments, exactly like the static `A.Pt{Float64}(...)` form.
# The base name `t` is only known at runtime, so the compile path previously
# failed with `Unknown parametric struct: t`. The fix applies the type
# parameters dynamically (`Core.apply_type`-style) and calls the resulting
# concrete `DataType`. This is the explicit-`{T}` analogue of the no-type-arg
# dynamic form `t(1.0, 2.0)` (Issue #8070): the implicit form INFERS the
# parameters, while the explicit form USES them and converts.

module A8101
    struct Pt8101{T}; x::T; y::T; end                 # 1 type parameter, same-T fields
    struct Foo8101{T}; a::T; b::Int; end              # a non-`T` field stays free
    export Pt8101, Foo8101
end

module B8101
    import ..A8101
    const Pt8101 = A8101.Pt8101                        # re-exported parametric alias
    export Pt8101
end
using .B8101

@testset "explicit apply-type on a local parametric DataType value (Issue #8101)" begin
    # MWE: local-var base, explicit `{Float64}`, arguments already Float64.
    t = A8101.Pt8101
    p = t{Float64}(1.0, 2.0)
    @test p.x == 1.0
    @test p.y == 2.0
    @test typeof(p) === A8101.Pt8101{Float64}
    @test p isa A8101.Pt8101{Float64}

    # Explicit `{T}` CONVERTS the arguments (Int → Float64), matching upstream's
    # `Pt8101{Float64}(1, 2.0)` semantics — it does NOT MethodError the way the
    # implicit `t(1, 2.0)` form does.
    pc = t{Float64}(1, 2.0)
    @test pc.x === 1.0
    @test pc.y === 2.0
    @test typeof(pc) === A8101.Pt8101{Float64}

    # A non-`T` field keeps its declared concrete type; only the `T` field is
    # substituted/converted.
    g = A8101.Foo8101
    q = g{Float64}(1, 2)
    @test q.a === 1.0
    @test q.b === 2
    @test typeof(q) === A8101.Foo8101{Float64}

    # The dynamic value is identical to the static one, and the static explicit
    # form still agrees.
    @test t === A8101.Pt8101
    @test typeof(A8101.Pt8101{Float64}(1, 2.0)) === A8101.Pt8101{Float64}

    # Re-exported `const Pt8101 = A8101.Pt8101` (via `using .B8101`) resolves to
    # the same parametric base under explicit apply-type.
    s = Pt8101
    sp = s{Float64}(3, 4)
    @test sp.x === 3.0
    @test sp.y === 4.0
    @test typeof(sp) === A8101.Pt8101{Float64}
end

true
