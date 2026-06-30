using Test

# Regression tests for Issues #3602, #3603, #3604:
# `startswith`, `endswith`, `occursin` previously mixed `length` (char count)
# with `codeunit` (byte index), producing false positives for non-ASCII
# inputs sharing leading/trailing UTF-8 bytes. Now uses `ncodeunits`.

@testset "startswith non-ASCII (#3602)" begin
    # MWE: distinct one-character non-ASCII prefixes sharing leading byte
    @test startswith("ê", "é") == false
    @test startswith("éx", "é") == true
    @test startswith("éxy", "ê") == false
    @test startswith("漢字", "漢") == true
    @test startswith("世界", "漢") == false

    # ASCII regression
    @test startswith("hello", "he") == true
    @test startswith("hello", "lo") == false
    @test startswith("hello", "") == true
    @test startswith("", "x") == false
    @test startswith("", "") == true
end

@testset "endswith non-ASCII (#3603)" begin
    # MWE
    @test endswith("ê", "é") == false
    @test endswith("xé", "é") == true
    @test endswith("xê", "é") == false
    @test endswith("漢字", "字") == true
    @test endswith("漢字", "漢") == false

    # ASCII regression
    @test endswith("hello", "lo") == true
    @test endswith("hello", "ho") == false
    @test endswith("hello", "") == true
end

@testset "occursin non-ASCII (#3604)" begin
    # MWE
    @test occursin("é", "ê") == false
    @test occursin("é", "xéy") == true
    @test occursin("é", "ééé") == true
    @test occursin("漢", "中漢字") == true
    @test occursin("漢", "字字字") == false

    # Substring of multi-char non-ASCII
    @test occursin("café", "Le café est chaud") == true
    @test occursin("kafe", "Le café est chaud") == false

    # ASCII regression
    @test occursin("ll", "hello") == true
    @test occursin("xy", "hello") == false
    @test occursin("", "hello") == true
end

true
