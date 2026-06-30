# Test split(s, "") on non-ASCII strings (Issue #3597)
# Empty delimiter should split by character, not by UTF-8 byte.

using Test

@testset "split(s, \"\") preserves multi-byte characters" begin
    # 2-byte UTF-8 characters (Latin-1 supplement)
    @test split("éa", "") == ["é", "a"]
    @test split("aé", "") == ["a", "é"]
    @test split("éé", "") == ["é", "é"]

    # 2-byte Greek
    @test split("αβγ", "") == ["α", "β", "γ"]

    # 3-byte CJK
    @test split("日本語", "") == ["日", "本", "語"]

    # 4-byte emoji (supplementary plane)
    @test split("a😀b", "") == ["a", "😀", "b"]

    # ASCII still correct
    @test split("abc", "") == ["a", "b", "c"]

    # Limit interacts correctly with multi-byte chars
    @test split("éaβ", ""; limit=2) == ["é", "aβ"]
    @test split("éaβ", ""; limit=1) == ["éaβ"]

    # Each output element has length 1 (one character)
    parts = split("éa", "")
    @test length(parts) == 2
    @test length(parts[1]) == 1
    @test length(parts[2]) == 1
end

true
