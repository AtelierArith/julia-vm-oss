# Kept standalone: overrides Base methods on Base argument types, so the method
# table interaction is process-global and aggregation is order-dependent even
# under upstream julia (#5966 class; excluded from Issue #10238 module-wrap
# aggregation).
using Test

import Base: deleteat!, insert!, pop!, popfirst!, push!, pushfirst!

push!(a::Vector{Int64}, x::Int64) = :push_override_4276
pop!(a::Vector{Int64}) = :pop_override_4276
pushfirst!(a::Vector{Int64}, x::Int64) = :pushfirst_override_4276
popfirst!(a::Vector{Int64}) = :popfirst_override_4276
insert!(a::Vector{Int64}, i::Int64, x::Int64) = :insert_override_4276
deleteat!(a::Vector{Int64}, i::Int64) = :deleteat_override_4276

runtime_push_4276(a::Any) = push!(a, 2)
runtime_pop_4276(a::Any) = pop!(a)
runtime_pushfirst_4276(a::Any) = pushfirst!(a, 2)
runtime_popfirst_4276(a::Any) = popfirst!(a)
runtime_insert_4276(a::Any) = insert!(a, 1, 2)
runtime_deleteat_4276(a::Any) = deleteat!(a, 1)

@testset "collection mutation user methods before fallback (Issue #4276)" begin
    @test push!([1], 2) == :push_override_4276
    @test runtime_push_4276([1]) == :push_override_4276
    anyv = Any[1]
    pushed_anyv = push!(anyv, 2)
    @test length(pushed_anyv) == 2
    @test pushed_anyv[1] == 1
    @test pushed_anyv[2] == 2
    @test length(anyv) == 2
    @test anyv[1] == 1
    @test anyv[2] == 2
    anyv2 = Any[1]
    pushed_anyv2 = runtime_push_4276(anyv2)
    @test length(pushed_anyv2) == 2
    @test pushed_anyv2[1] == 1
    @test pushed_anyv2[2] == 2
    @test length(anyv2) == 2
    @test anyv2[1] == 1
    @test anyv2[2] == 2

    @test pop!([1]) == :pop_override_4276
    @test runtime_pop_4276([1]) == :pop_override_4276
    anyp = Any[1, 2]
    @test pop!(anyp) == 2
    @test length(anyp) == 1
    @test anyp[1] == 1
    anyp2 = Any[1, 2]
    @test runtime_pop_4276(anyp2) == 2
    @test length(anyp2) == 1
    @test anyp2[1] == 1

    @test pushfirst!([1], 2) == :pushfirst_override_4276
    @test runtime_pushfirst_4276([1]) == :pushfirst_override_4276
    anyv3 = Any[1]
    pushed_first_anyv3 = pushfirst!(anyv3, 2)
    @test length(pushed_first_anyv3) == 2
    @test pushed_first_anyv3[1] == 2
    @test pushed_first_anyv3[2] == 1
    @test length(anyv3) == 2
    @test anyv3[1] == 2
    @test anyv3[2] == 1
    anyv4 = Any[1]
    pushed_first_anyv4 = runtime_pushfirst_4276(anyv4)
    @test length(pushed_first_anyv4) == 2
    @test pushed_first_anyv4[1] == 2
    @test pushed_first_anyv4[2] == 1
    @test length(anyv4) == 2
    @test anyv4[1] == 2
    @test anyv4[2] == 1

    @test popfirst!([1]) == :popfirst_override_4276
    @test runtime_popfirst_4276([1]) == :popfirst_override_4276
    anypf = Any[1, 2]
    @test popfirst!(anypf) == 1
    @test length(anypf) == 1
    @test anypf[1] == 2
    anypf2 = Any[1, 2]
    @test runtime_popfirst_4276(anypf2) == 1
    @test length(anypf2) == 1
    @test anypf2[1] == 2

    @test insert!([1], 1, 2) == :insert_override_4276
    @test runtime_insert_4276([1]) == :insert_override_4276
    anyv5 = Any[1]
    inserted_anyv5 = insert!(anyv5, 1, 2)
    @test length(inserted_anyv5) == 2
    @test inserted_anyv5[1] == 2
    @test inserted_anyv5[2] == 1
    @test length(anyv5) == 2
    @test anyv5[1] == 2
    @test anyv5[2] == 1
    anyv6 = Any[1]
    inserted_anyv6 = runtime_insert_4276(anyv6)
    @test length(inserted_anyv6) == 2
    @test inserted_anyv6[1] == 2
    @test inserted_anyv6[2] == 1
    @test length(anyv6) == 2
    @test anyv6[1] == 2
    @test anyv6[2] == 1

    @test deleteat!([1], 1) == :deleteat_override_4276
    @test runtime_deleteat_4276([1]) == :deleteat_override_4276
    anyv7 = Any[1, 2]
    deleted_anyv7 = deleteat!(anyv7, 1)
    @test length(deleted_anyv7) == 1
    @test deleted_anyv7[1] == 2
    @test length(anyv7) == 1
    @test anyv7[1] == 2
    anyv8 = Any[1, 2]
    deleted_anyv8 = runtime_deleteat_4276(anyv8)
    @test length(deleted_anyv8) == 1
    @test deleted_anyv8[1] == 2
    @test length(anyv8) == 1
    @test anyv8[1] == 2

    fv = [1.0]
    @test push!(fv, 2.0) == [1.0, 2.0]
    @test fv == [1.0, 2.0]

    fv2 = [1.0, 2.0]
    @test pop!(fv2) == 2.0
    @test fv2 == [1.0]

    fv3 = [1.0]
    @test pushfirst!(fv3, 2.0) == [2.0, 1.0]
    @test fv3 == [2.0, 1.0]

    fv4 = [1.0, 2.0]
    @test popfirst!(fv4) == 1.0
    @test fv4 == [2.0]

    fv5 = [1.0]
    @test insert!(fv5, 1, 2.0) == [2.0, 1.0]
    @test fv5 == [2.0, 1.0]

    fv6 = [1.0, 2.0]
    @test deleteat!(fv6, 1) == [2.0]
    @test fv6 == [2.0]
end

true
