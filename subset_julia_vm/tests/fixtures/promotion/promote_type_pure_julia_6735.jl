# Issue #6735 (#6727-1): promote_type's numeric promotion is driven by the
# registered promote_rule network (base/promotion.jl), not a hardcoded Rust
# priority table. The compile-time `type_priority` table in compile/promotion.rs
# was removed; numeric pairs resolve through the promote_rule registry, with the
# shared inference_core::PrimitiveNumeric taxonomy as the cache-less bootstrap
# fallback. Values verified against upstream julia 1.12.
#
# NOTE on user promote_rule extension: the promote_rule network is the dispatch
# path (so promote_type is user-extensible — promote_type(Meter,Foot)==Meter holds
# for a user-defined promote_rule(::Type{Meter},::Type{Foot})). The coexistence of
# a user promote_rule method with the numeric checks below is covered by
# promotion/user_promote_rule_coexists_6782.jl (Issue #6782 fixed the runtime
# method-table fence that previously corrupted unrelated numeric pairs); this
# fixture keeps the numeric-only checks isolated.

using Test

@testset "numeric promote_type via promote_rule network (Issue #6735)" begin
    @test promote_type(Int8, Int16) == Int16
    @test promote_type(Int16, Int8) == Int16
    @test promote_type(Int8, UInt8) == UInt8       # same width: unsigned wins
    @test promote_type(Int16, UInt16) == UInt16
    @test promote_type(Int32, Int64) == Int64
    @test promote_type(Float32, Float64) == Float64  # Float×Float via taxonomy
    @test promote_type(Float16, Float32) == Float32
    @test promote_type(Float16, Float64) == Float64
    @test promote_type(Int64, Float64) == Float64
    @test promote_type(Bool, Int64) == Int64
    @test promote_type(Bool, Float64) == Float64
    @test promote_type(Complex{Float64}, Int64) == Complex{Float64}
    @test promote_type(Int32, Int32) == Int32
end

@testset "Bottom / identity edge cases (Issue #6735)" begin
    @test promote_type(Int64, Int64) == Int64
    @test promote_type(Union{}, Int64) == Int64
    @test promote_type(Int64, Union{}) == Int64
end

true
