# Issue #3908: `String(::Vector{Char})`, `String` on Array-wrapper Char carriers,
# `codeunits(::String)`, `_substring_retag` on `split` results, and
# `findall(::String, ::String)` keep behaving correctly after the
# `builtins_strings.rs` Array-arm refactor that routes native Array char
# reads through the shared `try_chars_to_string_from_array_like` helper and
# the file-local `array_value` push helper.

let
    # String(::Vector{Char}) — covers the native Array Char carrier arm.
    chars = ['h', 'e', 'l', 'l', 'o']
    s = String(chars)
    @assert s == "hello"
    @assert length(s) == 5

    # String constructor on a reshaped Vector{Char} preserves logical order
    # via `ArrayValue::get_linear`.
    chars_row = reshape(['j', 'u', 'l', 'i', 'a'], 1, 5)
    @assert String(vec(chars_row)) == "julia"

    # Unicode characters round-trip through the helper.
    accent = ['é', 'ä', 'ü']
    @assert String(accent) == "éäü"

    # codeunits returns a CodeUnits wrapper with the right bytes.
    cu = codeunits("abc")
    @assert eltype(cu) === UInt8
    @assert length(cu) == 3
    @assert cu[1] == 0x61
    @assert cu[2] == 0x62
    @assert cu[3] == 0x63

    # `split` followed by `String(::SubString{String})` round-tripping exercises
    # the `_substring_retag` re-push through `array_value`.
    parts = split("a,b,c", ',')
    @assert length(parts) == 3
    @assert String(parts[1]) == "a"
    @assert String(parts[2]) == "b"
    @assert String(parts[3]) == "c"

    # findall returns a Vector{UnitRange{Int64}} via the
    # `array_value(new_array_ref(...))` push site.
    ranges = findall("ab", "ababab")
    @assert length(ranges) == 3
    @assert first(ranges[1]) == 1 && last(ranges[1]) == 2
    @assert first(ranges[2]) == 3 && last(ranges[2]) == 4
    @assert first(ranges[3]) == 5 && last(ranges[3]) == 6
end

true
