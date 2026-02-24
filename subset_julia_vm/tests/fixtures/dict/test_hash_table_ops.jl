# Test hash table helper functions (Issue #2747)
# Verifies _tablesz, _shorthash7, hashindex produce correct results
# matching Julia's official hash table algorithm.

using Test

@testset "Hash table helpers - _tablesz" begin
    # _tablesz rounds up to next power of 2, minimum 16
    # Reference: julia/base/abstractdict.jl:580
    @test _tablesz(0) == 16
    @test _tablesz(1) == 16
    @test _tablesz(8) == 16
    @test _tablesz(15) == 16
    @test _tablesz(16) == 16
    @test _tablesz(17) == 32
    @test _tablesz(31) == 32
    @test _tablesz(32) == 32
    @test _tablesz(33) == 64
    @test _tablesz(64) == 64
    @test _tablesz(65) == 128
    @test _tablesz(100) == 128
    @test _tablesz(128) == 128
    @test _tablesz(129) == 256
    @test _tablesz(256) == 256
    @test _tablesz(1000) == 1024
    @test _tablesz(1024) == 1024
    @test _tablesz(1025) == 2048
end

@testset "Hash table helpers - _shorthash7" begin
    # _shorthash7 extracts 7 MSBs from hash and sets bit 7
    # Result should always be in range 128-255
    sh = _shorthash7(0)
    @test sh >= 128
    @test sh <= 255

    # All zeros hash → only bit 7 set → 128
    @test _shorthash7(0) == 128

    # Hash with all bits set → (all 1s >>> 57) | 128 = 127 | 128 = 255
    @test _shorthash7(-1) == 255

    # Verify range for various inputs
    for val in [1, 42, 100, 1000, 99999]
        sh = _shorthash7(hash(val))
        @test sh >= 128
        @test sh <= 255
    end
end

@testset "Hash table helpers - hashindex" begin
    # hashindex returns (1-based index, shorthash7)
    # Index should be in range 1..sz
    sz = 16
    for key in [1, 2, 3, "a", "b", "hello"]
        idx, sh = hashindex(key, sz)
        @test idx >= 1
        @test idx <= sz
        @test sh >= 128
        @test sh <= 255
    end

    # Same key should always produce same index
    idx1, sh1 = hashindex("test", 32)
    idx2, sh2 = hashindex("test", 32)
    @test idx1 == idx2
    @test sh1 == sh2

    # Different sizes can give different indices
    idx_small, _ = hashindex("key", 16)
    @test idx_small >= 1
    @test idx_small <= 16

    idx_large, _ = hashindex("key", 1024)
    @test idx_large >= 1
    @test idx_large <= 1024
end

@testset "Hash table helpers - constants" begin
    @test maxallowedprobe == 16
    @test maxprobeshift == 6
end

true
