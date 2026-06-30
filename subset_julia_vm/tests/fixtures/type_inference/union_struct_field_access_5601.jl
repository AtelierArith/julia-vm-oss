# Issue #5601: field access over a Union of struct types must infer the join of
# the per-member field types (upstream `getfield_tfunc` over a Union), instead
# of widening to `Any`. Previously `(x::Union{A,B}).n` inferred `Any` whenever
# the object was a Union of two-or-more struct types — only struct+primitive
# unions narrowed. A member that lacks the field would throw at runtime, so it
# contributes `Bottom` to the join (matching upstream): the result is the join
# over the members that DO have the field.

using Test

struct UA5601; n::Int64; end
struct UB5601; n::Int64; end
struct UC5601; n::Float64; end
struct UD5601; m::Int64; end

# both members share the field type → that type
f_same(x::Union{UA5601,UB5601}) = x.n

# divergent field types → their Union join
f_mixed(x::Union{UA5601,UC5601}) = x.n

# isa-narrowed body: the issue's representative shape
function f_narrow(x::Union{UA5601,UB5601})
    if x isa UA5601
        return x.n
    end
    return 0
end

# only one member has the field; the other would throw → join over the one
f_partial(x::Union{UA5601,UD5601}) = x.n

# three-member all-struct union
f_three(x::Union{UA5601,UB5601,UC5601}) = x.n

@testset "Union-of-structs field access inference (Issue #5601)" begin
    @test Base.infer_return_type(f_same, Tuple{Union{UA5601,UB5601}}) == Int64
    @test Base.infer_return_type(f_mixed, Tuple{Union{UA5601,UC5601}}) == Union{Int64,Float64}
    @test Base.infer_return_type(f_narrow, Tuple{Union{UA5601,UB5601}}) == Int64
    @test Base.infer_return_type(f_partial, Tuple{Union{UA5601,UD5601}}) == Int64
    @test Base.infer_return_type(f_three, Tuple{Union{UA5601,UB5601,UC5601}}) == Union{Int64,Float64}

    # the values still compute correctly at runtime
    @test f_same(UA5601(7)) == 7
    @test f_mixed(UC5601(2.5)) == 2.5
    @test f_narrow(UB5601(9)) == 0
    @test f_narrow(UA5601(9)) == 9
end

true
