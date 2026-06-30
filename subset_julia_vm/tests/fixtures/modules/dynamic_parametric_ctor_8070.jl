using Test

# Issue #8070: dynamically calling the `DataType` VALUE of a PARAMETRIC struct
# (`t = A.Pt; t(1.0, 2.0)`) must infer the type parameters from the argument
# values and construct the parametric instance, just like the static
# `A.Pt(1.0, 2.0)` form. A parametric base is registered only in
# `parametric_structs` (no concrete `struct_defs` row until instantiated), so the
# dynamic-call fallback `try_construct_default_datatype` previously failed with
# "Function 'A.Pt' not found". The fix mirrors the compile-time default
# constructor's type-argument inference at runtime. This generalizes #8058
# (concrete default-field-constructor structs) to the parametric case.

module A8070
    struct Pt8070{T}; x::T; y::T; end                # 1 type parameter, same-T fields
    struct Pair8070{S, U}; a::S; b::U; end           # 2 independent type parameters
    struct Box8070{T}; v::T; end                     # single field, struct-valued ok
    export Pt8070, Pair8070, Box8070
end

module B8070
    import ..A8070
    const Pt8070 = A8070.Pt8070                       # re-exported parametric alias
    export Pt8070
end
using .B8070

@testset "dynamic call of a parametric struct's DataType value (Issue #8070)" begin
    # MWE: dynamic local-var call infers T from the arguments.
    t = A8070.Pt8070
    p = t(1.0, 2.0)
    @test p.x == 1.0
    @test p.y == 2.0
    @test typeof(p) === A8070.Pt8070{Float64}
    @test p isa A8070.Pt8070{Float64}

    # Different element type → different instantiation, still from the same value.
    q = t(3, 4)
    @test q.x == 3
    @test typeof(q) === A8070.Pt8070{Int64}

    # The dynamic value is identical to the static one.
    @test t === A8070.Pt8070

    # Two independent type parameters infer per-field.
    pr = A8070.Pair8070
    r = pr(1, 2.0)
    @test r.a == 1
    @test r.b == 2.0
    @test typeof(r) === A8070.Pair8070{Int64, Float64}
    @test typeof(pr("k", 5)) === A8070.Pair8070{String, Int64}

    # Single-parameter struct; the field value may itself be a struct.
    bx = A8070.Box8070
    @test typeof(bx(5)) === A8070.Box8070{Int64}
    @test typeof(bx(t(1.0, 2.0))) === A8070.Box8070{A8070.Pt8070{Float64}}

    # Re-exported `const Pt8070 = A8070.Pt8070` brought in via `using .B8070`
    # resolves to the same parametric base when called dynamically.
    s = Pt8070
    sp = s(1.5, 2.5)
    @test sp.x == 1.5
    @test typeof(sp) === A8070.Pt8070{Float64}
    @test s === A8070.Pt8070

    # Dynamic call on the qualified alias value still works.
    u = A8070.Pt8070
    @test u(7, 8).y == 8
end

true
