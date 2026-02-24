# Bool同士の比較 (Issue 292)

using Test

@testset "Bool-Bool comparison operators: ==, !=" begin
    a = true
    b = false

    # 基本的な比較
    @assert (a == a) == true
    @assert (b == b) == true
    @assert (a == b) == false
    @assert (a != b) == true

    # Bool値と直接比較（Issue 292の修正テスト）
    c = !true  # false
    @assert c == false

    # 変数同士の比較
    d = true
    e = true
    f = false
    @assert d == e
    @assert d != f

    @test (true)
end

true  # Test passed
