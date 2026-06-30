using Test

struct MemoryCompareBox3992
    x
end

@testset "struct field comparison reads Memory directly" begin
    m = Memory{Int64}(undef, 3)
    m[1] = 1
    m[2] = 2
    m[3] = 3

    m_diff = Memory{Int64}(undef, 3)
    m_diff[1] = 1
    m_diff[2] = 2
    m_diff[3] = 4

    mat = reshape([1, 2, 3], 3, 1)
    m_alias = m

    @test MemoryCompareBox3992(m) == MemoryCompareBox3992(m_alias)
    @test MemoryCompareBox3992(m) != MemoryCompareBox3992(m_diff)
    @test MemoryCompareBox3992(m) != MemoryCompareBox3992(mat)
end

true
