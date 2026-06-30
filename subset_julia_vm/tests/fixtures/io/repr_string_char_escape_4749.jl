using Test

@testset "repr(::String) escapes special chars to produce parseable literals (Issue #4749)" begin
    @test repr("hello") == "\"hello\""
    @test repr("") == "\"\""
    @test repr("with\nnewline") == "\"with\\nnewline\""
    @test repr("with\"quote") == "\"with\\\"quote\""
    @test repr("with\\backslash") == "\"with\\\\backslash\""
    @test repr("tab\there") == "\"tab\\there\""
end

@testset "repr(::Char) escapes special chars (Issue #4749)" begin
    @test repr('a') == "'a'"
    @test repr('\n') == "'\\n'"
    @test repr('\t') == "'\\t'"
    @test repr('\\') == "'\\\\'"
    @test repr('\'') == "'\\''"
    @test repr('"') == "'\"'"
end

@testset "print(::String) / string(::String) keep raw form (Issue #4749)" begin
    # print and string are the print-form (no escaping); only
    # show/repr add the escaping.
    @test string("a\nb") == "a\nb"
    @test string("a\"b") == "a\"b"
end

true
