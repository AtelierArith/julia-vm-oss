using Test

# Issue #4274: integer division / remainder helpers (`div`, `rem`, `mod`, `fld`,
# `cld`) over integer arguments throw `DivideError` (division by zero, or
# `typemin ÷ -1` overflow). Upstream `Base.infer_exception_type` reports
# `DivideError` and `Base.infer_effects` reports the total-except-`nothrow`
# record `(+c,+e,!n,+t,+s,+m,+u,+o,+r)` for these signatures. Before this slice
# sjulia collapsed them to the proven-total fallback (`Union{}` / all-true).
#
# The classification is keyed by name + all-`Integer` argument types so the
# float overloads (which never throw `DivideError`) and mixed int/float
# overloads keep falling through to the proven-total representative, exactly as
# upstream. `Bool <: Integer`, so `div(Bool, Bool)` is covered too. Values
# verified field-for-field against upstream Julia 1.12.6.

@testset "reflection integer div family exception type (#4274)" begin
    @test Base.infer_exception_type(div, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(rem, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(mod, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(fld, Tuple{Int64,Int64}) === DivideError
    @test Base.infer_exception_type(cld, Tuple{Int64,Int64}) === DivideError

    # Mixed integer widths and Bool are still all-`Integer`, so still DivideError.
    @test Base.infer_exception_type(div, Tuple{Int32,Int32}) === DivideError
    @test Base.infer_exception_type(mod, Tuple{Int64,Int32}) === DivideError
    @test Base.infer_exception_type(rem, Tuple{Int8,Int8}) === DivideError
    @test Base.infer_exception_type(div, Tuple{Bool,Bool}) === DivideError
end

@testset "reflection integer div family effects nothrow (#4274)" begin
    for f in (div, rem, mod, fld, cld)
        e = Base.infer_effects(f, Tuple{Int64,Int64})
        @test e.nothrow === false
        @test string(e) == "(+c,+e,!n,+t,+s,+m,+u,+o,+r)"
    end
end

@testset "reflection div family float overloads stay total (#4274)" begin
    # Float division never throws DivideError, so these keep the proven-total
    # representative with no inferred exception.
    for f in (div, rem, mod, fld, cld)
        @test Base.infer_exception_type(f, Tuple{Float64,Float64}) === Union{}
        @test string(Base.infer_effects(f, Tuple{Float64,Float64})) ==
            "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
    end
    # Mixed int/float is not all-`Integer`, so it also falls through to total.
    @test Base.infer_exception_type(mod, Tuple{Int64,Float64}) === Union{}
    @test string(Base.infer_effects(mod, Tuple{Int64,Float64})) ==
        "(+c,+e,+n,+t,+s,+m,+u,+o,+r)"
end

true
