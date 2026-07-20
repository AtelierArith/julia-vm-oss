using Test

# Issue #9035: the #9009 audit split two documented MustAlias precision
# limitations into their own tracking issue. Upstream Julia 1.12.6 keeps both
# shapes conservative:
#
#   * a mutable container `getindex` guard does not narrow the later element load
#   * a fresh alias after a field guard does not inherit the guarded path fact
#
# sjulia intentionally matches that compatibility boundary instead of inventing
# a stronger, potentially unsound inference rule. `--dump-bytecode` should keep
# these functions' inferred return path as `Union{Nothing, Int64}` rather than an
# Int64-only branch slot/load until an upstream-compatible MustAlias model exists.

struct MustAliasBox9035
    value::Union{Int64,Nothing}
end

function mutable_getindex_guard_9035(a::Vector{Union{Int64,Nothing}})
    if a[1] !== nothing
        return a[1]
    end
    return 0
end

function fresh_alias_field_guard_9035(x::MustAliasBox9035)
    if x.value !== nothing
        y = x
        return y.value
    end
    return 0
end

@testset "MustAlias narrowing compatibility boundary (Issue #9035)" begin
    @test Base.infer_return_type(
        mutable_getindex_guard_9035,
        Tuple{Vector{Union{Int64,Nothing}}},
    ) == Union{Nothing,Int64}
    @test Core.Compiler.return_type(
        mutable_getindex_guard_9035,
        Tuple{Vector{Union{Int64,Nothing}}},
    ) == Union{Nothing,Int64}

    @test Base.infer_return_type(
        fresh_alias_field_guard_9035,
        Tuple{MustAliasBox9035},
    ) == Union{Nothing,Int64}
    @test Core.Compiler.return_type(
        fresh_alias_field_guard_9035,
        Tuple{MustAliasBox9035},
    ) == Union{Nothing,Int64}

    @test mutable_getindex_guard_9035(Union{Int64,Nothing}[7, nothing]) == 7
    @test mutable_getindex_guard_9035(Union{Int64,Nothing}[nothing, 7]) == 0
    @test fresh_alias_field_guard_9035(MustAliasBox9035(7)) == 7
    @test fresh_alias_field_guard_9035(MustAliasBox9035(nothing)) == 0
end

true
