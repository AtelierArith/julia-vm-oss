# Issue #11642: chomp used length(s) (a character count) as a codeunit byte
# index, so multibyte content shifted the trailing-newline probe and the
# newline survived. Now mirrors upstream lastindex/prevind chomp.

@assert chomp("héllo\n") == "héllo"
@assert chomp("日本語\n") == "日本語"
@assert chomp("日本語\r\n") == "日本語"
@assert chomp("é\n") == "é"
@assert chomp("abc\n") == "abc"
@assert chomp("a\r\n") == "a"
# Upstream chomps only \n and \r\n — a lone trailing \r stays.
@assert chomp("a\r") == "a\r"
@assert chomp("\n") == ""
@assert chomp("\r\n") == ""
@assert chomp("") == ""
@assert chomp("no newline") == "no newline"
@assert ncodeunits(chomp("日本語\n")) == 9

println("All chomp multibyte tests passed")
true
