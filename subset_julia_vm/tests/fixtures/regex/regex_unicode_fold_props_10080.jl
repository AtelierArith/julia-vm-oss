using Test

# Issue #10080: PCRE2-vs-fancy-regex parity audit — Unicode areas that already
# match upstream Julia and must remain stable: case-insensitive folding of
# non-ASCII letters, \p{...} / \P{...} property classes, and \b / \w on
# non-ASCII text. Verified against upstream julia 1.12.

@testset "regex unicode case folding (Issue #10080)" begin
    # Simple case folding of non-ASCII letters under the i flag.
    @test occursin(r"é"i, "CAFÉ") == true
    @test match(r"ω"i, "Ω").match == "Ω"
    @test occursin(r"σ"i, "Σ") == true
    # Kelvin sign U+212A folds to k in both engines.
    @test occursin(r"k"i, "K") == true
    # Full case folding (ß -> ss) is NOT applied by either engine.
    @test occursin(r"STRASSE"i, "straße") == false
    # Dotted capital I U+0130 does not simple-fold to ASCII i in either engine.
    @test occursin(r"i"i, "İ") == false
    # ASCII classes still fold.
    @test occursin(r"[a-z]"i, "Q") == true
end

@testset "regex unicode property classes (Issue #10080)" begin
    @test occursin(r"\p{L}+", "abc") == true
    @test match(r"\p{L}+", "日本語123").match == "日本語"
    @test occursin(r"\p{Lu}", "aB") == true
    @test occursin(r"\p{Lu}", "ab") == false
    @test occursin(r"\p{Greek}", "αβ") == true
    @test occursin(r"\p{Greek}", "abc") == false
    @test occursin(r"\P{L}", "a1") == true
    @test occursin(r"\P{L}", "ab") == false
end

@testset "regex unicode word boundary and \\w (Issue #10080)" begin
    @test occursin(r"\bfoo\b", "a foo b") == true
    @test occursin(r"\bword\b", "wordy") == false
    # 日本 is followed by more word characters, so \b does not match there.
    @test occursin(r"\b日本\b", "これは日本です") == false
    # \w is Unicode-aware in both engines (UCP in PCRE2, default in fancy-regex).
    @test [m.match for m in eachmatch(r"\w+", "café naïve")] == ["café", "naïve"]
end

true
