using Test

# Bug fix: Char(n) must throw for n > 0x10FFFF instead of wrapping (Issue #3457)

@testset "strings_char_codepoint_bounds_valid" begin
    @test Char(0) == '\0'
    @test Char(65) == 'A'
    @test Char(0x10FFFF) == Char(1114111)
end

@testset "strings_char_codepoint_bounds_negative (Issue #3457)" begin
    @test_throws TypeError Char(-1)
end

@testset "strings_char_codepoint_bounds_above_unicode_max (Issue #3457)" begin
    @test_throws TypeError Char(0x110000)
end

@testset "strings_char_codepoint_bounds_above_u32_max (Issue #3457)" begin
    @test_throws TypeError Char(4294967296)
end

true
