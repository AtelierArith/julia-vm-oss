# Aggregated fixtures with top-level definitions, isolated by module wrapping
# (Issue #10238; Issue #9671 Phase 3 continuation, unblocked by the #9942 fix).
# Each block below is one former standalone fixture, verbatim except its
# trailing protocol `true`, wrapped in its own `module Agg_<stem>` so top-level
# struct/function/const/global definitions stay namespaced and cannot collide.
# `using Test` stays inside each module (modules do not inherit imports).
# @testset names (with their original Issue numbers) are preserved, and the
# #9360 @testset gate still detects any per-@testset failure.
# Source fixture in each banner.

# ===== source: strings/collect_string.jl =====
module Agg_collect_string
# collect(string) - collect string into character array (Issue #2027)

using Test

collect_runtime_any_string(x) = collect(x)
collect_trait_string(x) = Base._collect(1:1, x, Base.HasEltype(), Base.HasLength())

@testset "collect(string) into Char array (Issue #2027)" begin
    @test eltype("abc") === Char
    @test eltype(String) === Char

    # Basic string collection
    result = collect("abc")
    @test typeof(result) === Vector{Char}
    @test eltype(result) === Char
    @test length(result) == 3
    @test result[1] == 'a'
    @test result[2] == 'b'
    @test result[3] == 'c'

    runtime_result = collect_runtime_any_string("abc")
    @test typeof(runtime_result) === Vector{Char}
    @test eltype(runtime_result) === Char
    @test length(runtime_result) == 3
    @test runtime_result[1] == 'a'
    @test runtime_result[2] == 'b'
    @test runtime_result[3] == 'c'

    # Longer string
    result2 = collect("hello")
    @test typeof(result2) === Vector{Char}
    @test eltype(result2) === Char
    @test length(result2) == 5
    @test result2[1] == 'h'
    @test result2[5] == 'o'

    # Single character string
    result3 = collect("x")
    @test typeof(result3) === Vector{Char}
    @test eltype(result3) === Char
    @test length(result3) == 1
    @test result3[1] == 'x'

    # Empty string
    result4 = collect("")
    @test typeof(result4) === Vector{Char}
    @test eltype(result4) === Char
    @test length(result4) == 0

    runtime_empty = collect_runtime_any_string("")
    @test typeof(runtime_empty) === Vector{Char}
    @test eltype(runtime_empty) === Char
    @test length(runtime_empty) == 0

    # String with spaces
    result5 = collect("a b")
    @test typeof(result5) === Vector{Char}
    @test eltype(result5) === Char
    @test length(result5) == 3
    @test result5[1] == 'a'
    @test result5[2] == ' '
    @test result5[3] == 'b'

    # Multibyte Unicode iteration is character-based, not byte-based.
    result6 = collect("éβ")
    @test typeof(result6) === Vector{Char}
    @test eltype(result6) === Char
    @test length(result6) == 2
    @test result6[1] == 'é'
    @test result6[2] == 'β'
end

@testset "_collect HasEltype string trait path (Issue #4062)" begin
    result = Base._collect(1:1, "abc", Base.HasEltype(), Base.HasLength())
    @test typeof(result) === Vector{Char}
    @test eltype(result) === Char
    @test length(result) == 3
    @test String(result) == "abc"

    runtime_result = collect_trait_string("éβ")
    @test typeof(runtime_result) === Vector{Char}
    @test eltype(runtime_result) === Char
    @test length(runtime_result) == 2
    @test String(runtime_result) == "éβ"

    empty_result = Base._collect(1:1, "", Base.IteratorEltype(""), Base.IteratorSize(""))
    @test typeof(empty_result) === Vector{Char}
    @test eltype(empty_result) === Char
    @test length(empty_result) == 0
end
end # module Agg_collect_string

# ===== source: strings/raw_string_literals.jl =====
module Agg_raw_string_literals
# Test raw string literals (Issue #554)
# Tests the raw"..." string macro literal
# In Julia, raw strings process \\ (to single \) and \" (to ")
# but keep other escape sequences like \n as literal backslash+letter

using Test

# Test 1: raw string preserves backslash+letter sequences
@test raw"\n\t" == "\\n\\t"
@test length(raw"\n\t") == 4

# Test 2: raw string processes double backslash to single backslash
# raw"\\\\" has 4 backslashes in source -> 2 backslashes
@test raw"\\\\" == "\\\\"
@test length(raw"\\\\") == 2

# Test 3: raw string with normal text
@test raw"hello" == "hello"
@test raw"hello world" == "hello world"

# Test 4: raw string with special characters that would normally be escapes
@test raw"\a\b\f\r\v" == "\\a\\b\\f\\r\\v"

# Test 5: raw string in function
function get_raw_string()
    return raw"\path\to\file"
end
@test get_raw_string() == "\\path\\to\\file"

# Test 6: raw string comparison
x = raw"\n"
y = "\\n"
@test x == y

# Test 7: single backslash
@test length(raw"\\") == 1

# Return true to indicate success
end # module Agg_raw_string_literals

# ===== source: strings/string_comparison_dynamic.jl =====
module Agg_string_comparison_dynamic
# Test string comparison with dynamic dispatch (Issue #1218)
# Verifies that string == string returns Bool when one operand has type Any at compile time

using Test

# Helper struct and functions to create dynamic string values
struct StringHolder
    msg::String
end

function get_msg(s::StringHolder)
    return s.msg
end

function inner_string()
    return "hello"
end

function outer_string()
    return inner_string()
end

@testset "String comparison with dynamic dispatch" begin
    # Test 1: Direct string comparison (both types known at compile time)
    @testset "Static string comparison" begin
        a = "hello"
        b = "hello"
        c = "world"
        @test a == b
        @test !(a == c)
        @test a != c
    end

    # Test 2: Struct field string comparison (type becomes Any through function call)
    @testset "Struct field string comparison" begin
        holder = StringHolder("hello")
        result = get_msg(holder)
        @test result == "hello"
        @test !(result == "world")
        @test result != "world"
    end

    # Test 3: Nested function call string comparison
    @testset "Nested function string comparison" begin
        result = outer_string()
        @test result == "hello"
        @test !(result == "world")
    end

    # Test 4: Both operands are dynamic
    @testset "Both operands dynamic" begin
        h1 = StringHolder("test")
        h2 = StringHolder("test")
        h3 = StringHolder("other")
        r1 = get_msg(h1)
        r2 = get_msg(h2)
        r3 = get_msg(h3)
        @test r1 == r2
        @test !(r1 == r3)
        @test r1 != r3
    end

    # Test 5: String inequality (!=) with dynamic dispatch
    @testset "String inequality dynamic" begin
        holder = StringHolder("hello")
        result = get_msg(holder)
        @test result != "world"
        @test !(result != "hello")
    end
end
end # module Agg_string_comparison_dynamic

# ===== source: strings/string_findfirst_non_ascii.jl =====
module Agg_string_findfirst_non_ascii
using Test

# Regression tests for Issue #3605:
# `findfirst(::String, ::String)` (and `findnext`/`findlast`/`findprev`)
# previously returned the byte-length range `i:i+ncodeunits(pattern)-1`.
# For non-ASCII patterns this overshoots — Julia's returned UnitRange ends
# at the byte index of the *start* of the last matched character, so e.g.
# `findfirst("é", "éa")` should be `1:1`, not `1:2`.

# UnitRange `==` is not yet implemented in subset Julia VM, so compare
# endpoints with `first`/`last` (matching `string_findnext_findprev.jl`).

function range_eq(r, lo, hi)
    return first(r) == lo && last(r) == hi
end

@testset "findfirst non-ASCII range (#3605)" begin
    # MWE: one-character non-ASCII pattern (2 bytes)
    @test range_eq(findfirst("é", "éa"), 1, 1)
    @test range_eq(findfirst("é", "aé"), 2, 2)
    @test range_eq(findfirst("é", "ééé"), 1, 1)

    # Multi-character non-ASCII pattern: end is byte index of last char start.
    # "éa" has chars at bytes 1 and 3 within the pattern, so the range spans
    # `i:(i+2)`.
    @test range_eq(findfirst("éa", "béa"), 2, 4)
    @test range_eq(findfirst("éé", "aéé"), 2, 4)

    # 3-byte chars (CJK)
    @test range_eq(findfirst("漢", "中漢字"), 4, 4)
    @test range_eq(findfirst("漢字", "中漢字"), 4, 7)
    @test range_eq(findfirst("中", "中漢字"), 1, 1)

    # ASCII regression: byte length and char-boundary length coincide.
    @test range_eq(findfirst("a", "abc"), 1, 1)
    @test range_eq(findfirst("ab", "xab"), 2, 3)
    @test range_eq(findfirst("bc", "abcabc"), 2, 3)
    @test findfirst("xyz", "abcabc") === nothing

    # Empty pattern: `i:i-1` (1:0)
    @test range_eq(findfirst("", "abc"), 1, 0)
end

@testset "findlast non-ASCII range (#3605)" begin
    @test range_eq(findlast("é", "aée"), 2, 2)
    @test range_eq(findlast("é", "ééé"), 5, 5)
    @test range_eq(findlast("éé", "aéé"), 2, 4)
    @test range_eq(findlast("漢", "中漢字"), 4, 4)
    @test range_eq(findlast("a", "abcabc"), 4, 4)
    @test range_eq(findlast("ab", "abcabc"), 4, 5)
    @test findlast("xyz", "abcabc") === nothing
end

@testset "findnext/findprev non-ASCII range (#3605)" begin
    # Resume search at the next valid string index (byte index of a char start)
    # "éaé": chars at bytes 1, 3, 4 — start search at 3 to skip the first 'é'.
    @test range_eq(findnext("é", "éaé", 3), 4, 4)
    # "aéaé": chars at bytes 1, 2, 4, 5 — search past the last 'é' returns nothing.
    @test findnext("é", "aéaé", 7) === nothing

    # "éaéa": chars at bytes 1, 3, 4, 6 — search backwards from byte 4 ('é').
    @test range_eq(findprev("é", "éaéa", 4), 4, 4)
    @test range_eq(findprev("é", "éaéa", 3), 1, 1)
end
end # module Agg_string_findfirst_non_ascii

# ===== source: strings/string_slice_concat.jl =====
module Agg_string_slice_concat
using Test

# Regression tests for Issue #3671:
# Inside a function body, the binary `*` operator falls through to
# `dynamic_mul`, which previously had no case for `Value::Str * Value::Str`
# (or Char). Concatenating a slice result with another String produced
# `Cannot multiply "String" and "String"` even though both operands were
# concretely `String`. Same path also broke `String * Char` and `Char * Char`.

function _slice_concat(s)
    result = ""
    chunk = s[2:4]
    return result * chunk
end

function _string_times_char(s)
    return "" * s[1]
end

function _char_times_string(s)
    return s[1] * ""
end

function _char_times_char(s)
    return s[1] * s[2]
end

function _slice_times_slice(s)
    return s[1:2] * s[3:4]
end

function _slice_times_literal(s)
    return s[2:4] * "!"
end

@testset "String slice * String concat (#3671)" begin
    @test _slice_concat("hello") == "ell"
    @test _slice_times_slice("hello") == "hell"
    @test _slice_times_literal("hello") == "ell!"

    # Non-ASCII slice: char positions in "aéabcd" are 1, 2, 4, 5, 6, 7.
    # `s[2:4]` covers 'é' (bytes 2..3) and 'a' (byte 4) → "éa".
    @test _slice_concat("aéabcd") == "éa"
end

@testset "String * Char and Char * String (#3671)" begin
    @test _string_times_char("hello") == "h"
    @test _char_times_string("hello") == "h"
    @test _char_times_char("hello") == "he"

    # Non-ASCII chars
    @test _string_times_char("éhi") == "é"
    # "aéh": char positions 1, 2, 4 — s[1] = 'a', s[2] = 'é'.
    @test _char_times_char("aéh") == "aé"
end

@testset "Plain string concat regression" begin
    # These already worked; ensure the new dynamic_mul cases don't regress them.
    @test "a" * "b" == "ab"
    @test "ab" * "cd" == "abcd"
    @test "" * "hello" == "hello"
    @test "hello" * "" == "hello"
end
end # module Agg_string_slice_concat

# ===== source: strings/string_substring_vector_display.jl =====
module Agg_string_substring_vector_display
using Test

# Regression tests for Issue #3574:
# `split`/`rsplit` previously returned `Vector{String}` and rendered as
# `["a", "b"]` (no element-type prefix) — Julia 1.12 returns
# `Vector{SubString{String}}` and renders as `SubString{String}["a", "b"]`.
# The VM doesn't have a separate substring runtime type; the result array is
# tagged via `_substring_retag` so `typeof`, `eltype`, and `show` match Julia.

# Helper: capture the `print` (== `string`) form of a value the same way
# the issue's MWE checks (`println(split(...))`).
_show(x) = sprint(print, x)

@testset "split show form (#3574)" begin
    @test _show(split("a,b", ",")) == "SubString{String}[\"a\", \"b\"]"
    @test _show(split("a,b,c", ",")) == "SubString{String}[\"a\", \"b\", \"c\"]"
    @test _show(split("a,,b", ",")) == "SubString{String}[\"a\", \"\", \"b\"]"
    @test _show(split("a,,b", ","; keepempty=false)) == "SubString{String}[\"a\", \"b\"]"
    @test _show(split("hello world")) == "SubString{String}[\"hello\", \"world\"]"

    # Char delimiter — kwarg variants delegate to the String form, which retags.
    @test _show(split("a-b-c", '-')) == "SubString{String}[\"a\", \"b\", \"c\"]"
end

@testset "rsplit show form (#3574)" begin
    @test _show(rsplit("a,b,c", ",")) == "SubString{String}[\"a\", \"b\", \"c\"]"
    @test _show(rsplit("a,b,c", ","; limit=2)) == "SubString{String}[\"a,b\", \"c\"]"
    @test _show(rsplit("a-b", '-')) == "SubString{String}[\"a\", \"b\"]"
end

@testset "split typeof / eltype (#3574)" begin
    @test string(typeof(split("a,b", ","))) == "Vector{SubString{String}}"
    @test string(eltype(split("a,b", ","))) == "SubString{String}"

    # rsplit too
    @test string(typeof(rsplit("a,b", ","))) == "Vector{SubString{String}}"
    @test string(eltype(rsplit("a,b", ","))) == "SubString{String}"
end

@testset "non-ASCII split show (#3574)" begin
    # Multi-byte UTF-8 chars in the splitting result still render correctly.
    @test _show(split("aé,bê,cé", ",")) == "SubString{String}[\"aé\", \"bê\", \"cé\"]"
end

@testset "Vector{String} literal stays bare (#3574)" begin
    # Array literals do NOT get the SubString tag — only split/rsplit results do.
    # In Julia and the VM, `["a", "b"]` shows as `["a", "b"]`.
    @test _show(["a", "b"]) == "[\"a\", \"b\"]"
end
end # module Agg_string_substring_vector_display

true
