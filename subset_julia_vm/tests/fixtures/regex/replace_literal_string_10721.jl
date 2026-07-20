# Issue #10721: replace(s, re => "plain \$1") with a PLAIN String replacement
# wrongly expanded $-capture references (the Rust regex engine's replacement
# syntax leaked through _regex_replace). Upstream treats a plain String
# replacement as literal text; only a SubstitutionString (s"...") expands
# capture references (via \N, not $N).

using Test

@testset "plain String replacement is literal (Issue #10721)" begin
    @test replace("abc", r"b" => "plain \$1") == "aplain \$1c"
    @test replace("abc", r"b" => "x\$y") == "ax\$yc"
    @test replace("abc", r"b" => "\$0") == "a\$0c"
    # SubstitutionString still expands its \N capture references.
    @test replace("abc", r"(b)" => s"got \1") == "agot bc"
    # Ordinary replacements unaffected.
    @test replace("hello world", r"o" => "0") == "hell0 w0rld"
    @test replace("aaa", r"a" => "b"; count=2) == "bba"
end

true
