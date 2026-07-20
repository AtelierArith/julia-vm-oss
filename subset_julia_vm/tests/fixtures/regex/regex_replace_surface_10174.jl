using Test

# Issues #10174 / #10175 / #10197: replace() upstream-parity surface.
#
#   #10174  SubstitutionString (s"...") capture references (\1..\9, \g<name>,
#           \0 = whole match) are expanded against each match instead of copied
#           verbatim.
#   #10175  A Function replacement value (pat => f) is called per matched
#           substring, and multiple pattern pairs replace(s, p1, p2, ...) are
#           applied left-to-right simultaneously (both String and Regex
#           patterns).
#   #10197  count=0 replaces nothing (returns the string unchanged).
#
# All expected values verified against upstream julia 1.12.

@testset "replace SubstitutionString capture refs (Issue #10174)" begin
    @test replace("hello world", r"(\w+) (\w+)" => s"\2 \1") == "world hello"
    @test replace("2026-07-10", r"(?<y>\d+)-(?<m>\d+)-(?<d>\d+)" => s"\g<d>/\g<m>/\g<y>") == "10/07/2026"
    @test replace("abc", r"b" => s"[\0]") == "a[b]c"
    @test replace("The quick foxes", r"fox(es)?" => s"bus\1") == "The quick buses"
    # \0 (whole match) is also valid for a plain-String pattern.
    @test replace("abc", "b" => s"[\0]") == "a[b]c"
end

@testset "replace Function replacement value (Issue #10175)" begin
    @test replace("hello", r"l+" => uppercase) == "heLLo"
    @test replace("hello", "l" => uppercase) == "heLLo"
    @test replace("a1b2c3", r"\d" => (m -> "<" * m * ">")) == "a<1>b<2>c<3>"
    @test replace("xax", 'a' => uppercase) == "xAx"
end

@testset "replace multiple pattern pairs (Issue #10175)" begin
    @test replace("abc", r"a" => "x", r"c" => "y") == "xby"
    @test replace("abc", "a" => "x", "c" => "y") == "xby"
    # Applied simultaneously, left-to-right; patterns match only the input.
    @test replace("abcabc", "a" => "b", "b" => "c", r".+" => "a") == "bca"
    @test replace("hello world", r"o" => "0", r"l" => "L") == "heLL0 w0rLd"
    # Mixed replacement kinds across pairs.
    @test replace("a1b2", r"\d" => s"[\0]", r"[a-z]" => uppercase) == "A[1]B[2]"
end

@testset "replace count=0 returns string unchanged (Issue #10197)" begin
    @test replace("aaa", r"a" => "b"; count=0) == "aaa"
    @test replace("aaa", "a" => "b"; count=0) == "aaa"
    @test replace("hello", r"l" => uppercase; count=0) == "hello"
end

@testset "replace count with non-literal replacements" begin
    @test replace("aaa", r"a" => uppercase; count=2) == "AAa"
    @test replace("a1b2c3", r"\d" => s"<\0>"; count=2) == "a<1>b<2>c3"
end

@testset "replace SubstitutionString escapes (Issue #10174)" begin
    # C-escapes and hex/unicode escapes are unescaped like upstream's
    # unescape_string; only \N / \g<name> / \0 / \\ stay capture references.
    @test replace("z", r"z" => s"\x41") == "A"
    @test replace("z", r"z" => s"é") == "é"
    @test replace("z", "z" => s"a\tb") == "a\tb"
    @test replace("ab", r"(a)(b)" => s"\g<2>\g<1>") == "ba"
    # \g<0> (the whole match) is valid even for a non-Regex pattern.
    @test replace("xy", "x" => s"[\g<0>]") == "[x]y"
end

true
