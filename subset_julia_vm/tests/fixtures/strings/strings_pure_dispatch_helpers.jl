# Verify Pure Julia dispatch for residual string helpers (Issue #3726)
#
# - isvalid(s::String, i::Integer) was previously dispatched to
#   BuiltinId::IsvalidIndex via compile/expr/builtin_string.rs.
# - findall(pattern::String, s::String) and findall(c::Char, s::String)
#   were previously routed to BuiltinId::StringFindAll in
#   compile/expr/call/mod.rs.
# - count(pattern::String, s::String) and count(c::Char, s::String) were
#   previously routed to BuiltinId::StringCount.
#
# After Issue #3726, all of the above resolve to Pure Julia methods in
# subset_julia_vm/src/julia/base/strings/{basic.jl,search.jl}. Calls fall
# through method dispatch and the Rust builtins remain only as cache-
# compatibility fallbacks (no longer reachable from new IR).

using Test

@testset "isvalid Pure Julia dispatch" begin
    # Multibyte UTF-8 character: 'é' = 2 codeunits (0xC3 0xA9)
    @test isvalid("é", 1) == true
    @test isvalid("é", 2) == false  # continuation byte
    # Out-of-bounds
    @test isvalid("é", 0) == false
    @test isvalid("é", 3) == false
    @test isvalid("abc", -1) == false
    @test isvalid("abc", 4) == false
    # ASCII string: every byte is a valid boundary
    @test isvalid("abc", 1) == true
    @test isvalid("abc", 2) == true
    @test isvalid("abc", 3) == true
    # Empty string: no valid indices
    @test isvalid("", 1) == false
end

@testset "findall(pattern::String, s::String) Pure Julia dispatch" begin
    # Non-overlapping matches
    r = findall("ana", "banana")
    @test length(r) == 1
    @test first(r[1]) == 2
    @test last(r[1]) == 4

    # Multiple non-overlapping matches
    r2 = findall("aba", "abababa")
    @test length(r2) == 2
    @test first(r2[1]) == 1
    @test last(r2[1]) == 3
    @test first(r2[2]) == 5
    @test last(r2[2]) == 7

    # No matches
    r3 = findall("xyz", "banana")
    @test length(r3) == 0

    # Single character pattern (still String overload)
    r4 = findall("a", "banana")
    @test length(r4) == 3
    @test first(r4[1]) == 2
    @test first(r4[2]) == 4
    @test first(r4[3]) == 6
end

@testset "findall(c::Char, s::String) Pure Julia dispatch" begin
    @test findall('a', "banana") == [2, 4, 6]
    @test isempty(findall('z', "banana"))
    @test findall('b', "banana") == [1]
end

@testset "count(pattern::String, s::String) Pure Julia dispatch" begin
    @test count("ana", "banana") == 1  # non-overlapping
    @test count("aba", "abababa") == 2
    @test count("xyz", "banana") == 0
    @test count("b", "abc") == 1
    @test count("abc", "abc") == 1
    # Empty pattern: count("", s) == length(s) + 1
    @test count("", "abc") == 4
    @test count("", "") == 1
end

@testset "count(c::Char, s::String) Pure Julia dispatch" begin
    @test count('a', "banana") == 3
    @test count('z', "banana") == 0
    @test count('l', "hello world") == 3
end

true
