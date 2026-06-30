# Stress the internal dict-key hashing under resize and collision-prone
# patterns (Issue #5188). The VM swapped the internal open-addressing slot
# hash from SipHash (DefaultHasher) to a fast non-crypto FxHash. Slot
# positions are not observable, so these tests assert only behaviour that
# upstream Julia also guarantees: every inserted key round-trips, equal
# keys collide on the same entry, and delete/reinsert keeps integrity.
#
# Note: where a dict has String values, the looked-up value is bound to a
# local before comparison. Inlining `@test dict[k] == "lit"` trips an
# unrelated @test-macro bug (Issue #5269), so it is avoided here to keep the
# fixture focused on Issue #5188 and at parity with upstream Julia.

using Test

@testset "many integer keys with resize" begin
    d = Dict{Int,Int}()
    for i in 1:1000
        d[i] = i * 2
    end
    @test length(d) == 1000
    @test all(d[i] == i * 2 for i in 1:1000)
    # Overwrite existing keys; length stays the same.
    for i in 1:1000
        d[i] = i * 3
    end
    @test length(d) == 1000
    @test all(d[i] == i * 3 for i in 1:1000)
end

@testset "string keys with shared prefixes" begin
    d = Dict{String,Int}()
    for i in 1:1000
        d["key_" * string(i)] = i
    end
    @test length(d) == 1000
    @test all(d["key_" * string(i)] == i for i in 1:1000)
    @test !haskey(d, "key_0")
    @test haskey(d, "key_1000")
end

@testset "mixed numeric key equality collides on one entry" begin
    # In Julia, 1 == 1.0 == 0x1 as dict keys (isequal-based), so they all map
    # to the same entry regardless of the underlying slot hash.
    d = Dict{Any,Int}()
    d[1] = 10
    d[1.0] = 20
    d[0x1] = 30
    @test length(d) == 1
    @test d[1] == 30
    @test d[1.0] == 30
end

@testset "delete and reinsert keeps integrity" begin
    d = Dict{Int,Int}()
    for i in 1:500
        d[i] = i
    end
    for i in 1:2:500
        delete!(d, i)
    end
    @test length(d) == 250
    for i in 1:2:500
        @test !haskey(d, i)
    end
    for i in 2:2:500
        @test d[i] == i
    end
    # Reinsert the deleted keys with fresh values (may reuse deleted slots).
    for i in 1:2:500
        d[i] = i * 100
    end
    @test length(d) == 500
    @test all(d[i] == (isodd(i) ? i * 100 : i) for i in 1:500)
end

@testset "Set of mixed keys, resize-heavy" begin
    s = Set{Int}()
    for i in 1:2000
        push!(s, i)
    end
    @test length(s) == 2000
    @test all(i in s for i in 1:2000)
    @test !(2001 in s)
    # Pushing duplicates does not grow the set.
    for i in 1:2000
        push!(s, i)
    end
    @test length(s) == 2000
end

@testset "negative zero and NaN float keys" begin
    # -0.0 and 0.0 are distinct dict keys (not isequal); NaN keys are isequal
    # to themselves. The fast hash must preserve these contracts. String
    # values are read into locals before comparison (see header note).
    d = Dict{Float64,String}()
    d[0.0] = "pos"
    d[-0.0] = "neg"
    @test length(d) == 2
    vpos = d[0.0]
    vneg = d[-0.0]
    @test vpos == "pos"
    @test vneg == "neg"

    d2 = Dict{Float64,Int}()
    d2[NaN] = 1
    d2[NaN] = 2
    @test length(d2) == 1
    @test d2[NaN] == 2
end

true
