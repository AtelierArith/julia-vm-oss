# String-op typed-loop candidate benchmark (Issue #10559, split from #10477).
#
# Shape mirrors the existing typed-loop scalar benchmarks (calc_pi_n5000_typed_while.jl):
# a tight `while` loop with a fixed trip count, String-typed local slot reads/writes,
# and a scalar accumulator — the pattern a `StringConcat`-style typed-loop op would
# need to recognize. Used to measure whether a String slot class in the typed-loop
# IR would pay for itself once string allocation cost is included.

# 1. Small-string concatenation accumulation (grows to O(n) chars → O(n^2) work,
#    same shape upstream Julia code commonly writes; each `*` allocates a new String).
function concat_accum(n::Int64)::Int64
    s::String = ""
    i::Int64 = 0
    while i < n
        s = s * "x"
        i += 1
    end
    return length(s)
end

# 2. Fixed-length repeated concatenation (bounded allocation per iteration —
#    closer to what a typed-loop String slot could actually help with, since the
#    string never grows unboundedly).
function fixed_concat(n::Int64)::Int64
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        s::String = "abc" * "def"
        total += length(s)
        i += 1
    end
    return total
end

# 3. String comparison loop (no allocation — pure byte comparison, the cheapest
#    string op a typed loop could special-case).
function compare_loop(n::Int64)::Int64
    total::Int64 = 0
    i::Int64 = 0
    needle::String = "needle"
    while i < n
        if needle == "needle"
            total += 1
        end
        i += 1
    end
    return total
end

# 4. String indexing / iteration loop (byte-index correctness matters for UTF-8).
function index_loop(n::Int64)::Int64
    s::String = "abcdefghij"
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        total += Int64(s[(i % 10) + 1])
        i += 1
    end
    return total
end

println("concat_accum=", concat_accum(20000))
println("fixed_concat=", fixed_concat(200000))
println("compare_loop=", compare_loop(2000000))
println("index_loop=", index_loop(2000000))
