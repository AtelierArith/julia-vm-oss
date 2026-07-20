# Issue #10559: String slot reads/writes inside a recognized typed loop.
#
# Covers the typed-loop String ops (LoadStrSlot / StoreStrSlot / PushStrConst /
# ConcatStr / EqStr / StrLen) and — critically — their UTF-8 correctness:
# Julia's `String` is byte-indexed UTF-8, but `length(s)` counts CODEPOINTS, so
# a typed-loop `StrLen` that returned the byte length would be wrong on any
# non-ASCII input. Every assertion below is verified against upstream `julia`.

# 1. String accumulation (`s = s * "x"`) — the StringConcat shape.
function concat_accum(n::Int64)::Int64
    s::String = ""
    i::Int64 = 0
    while i < n
        s = s * "x"
        i += 1
    end
    return length(s)
end

# 2. Bounded concatenation of two String slots (params live in through the loop).
function concat_bounded(n::Int64, prefix::String, suffix::String)::Int64
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        s::String = prefix * suffix
        total += length(s)
        i += 1
    end
    return total
end

# 3. String equality inside the loop (EqStr; allocation-free).
function count_matches(n::Int64, needle::String)::Int64
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        if needle == "needle"
            total += 1
        end
        i += 1
    end
    return total
end

# 4. UTF-8 / multi-byte: `length` must be the CODEPOINT count, not the byte
#    count. "αβγ" is 3 codepoints / 6 bytes; "日本語" is 3 codepoints / 9 bytes;
#    "é" is 1 codepoint / 2 bytes. A byte-length typed-loop op would return
#    6 / 9 / 2 here and fail these assertions.
function unicode_len_loop(n::Int64, s::String)::Int64
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        total += length(s)
        i += 1
    end
    return total
end

# 5. UTF-8 accumulation: concatenating multi-byte strings must preserve both the
#    codepoint count and the exact bytes.
function unicode_concat(n::Int64)::String
    s::String = ""
    i::Int64 = 0
    while i < n
        s = s * "日本"
        i += 1
    end
    return s
end

@assert concat_accum(0) == 0
@assert concat_accum(1) == 1
@assert concat_accum(100) == 100

@assert concat_bounded(0, "abc", "def") == 0
@assert concat_bounded(10, "abc", "def") == 60
@assert concat_bounded(5, "", "") == 0

@assert count_matches(10, "needle") == 10
@assert count_matches(10, "haystack") == 0

# codepoints, not bytes
@assert unicode_len_loop(1, "αβγ") == 3
@assert unicode_len_loop(4, "αβγ") == 12
@assert unicode_len_loop(1, "日本語") == 3
@assert unicode_len_loop(1, "é") == 1
@assert unicode_len_loop(1, "aé日") == 3
@assert unicode_len_loop(1, "") == 0

@assert unicode_concat(0) == ""
@assert unicode_concat(1) == "日本"
@assert unicode_concat(3) == "日本日本日本"
@assert length(unicode_concat(3)) == 6
@assert ncodeunits(unicode_concat(3)) == 18

# The typed loop must agree with the interpreter on the exact bytes, so a
# round-trip through the codepoint count and the byte count both hold.
u::String = unicode_concat(2)
@assert length(u) == 4
@assert ncodeunits(u) == 12
@assert u == "日本日本"

println("string typed loop (Issue #10559): OK")

true
