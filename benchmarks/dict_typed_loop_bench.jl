# Dict-op typed-loop candidate benchmark (Issue #10560, split from #10477 item 3).
#
# Shape mirrors benchmarks/string_typed_loop_bench.jl (Issue #10559): tight
# `while` loops with a fixed trip count exercising haskey / getindex /
# setindex! on a Dict{Int64,Int64}, the pattern a Dict-op typed-loop
# recognizer would need to natively execute. Used to measure whether a Dict
# handle class in the typed-loop IR would pay for itself, given that (unlike
# Value::Str, which is Rc<str> and thus a cheap slot handle) a live Dict is a
# StructRef into struct_heap whose slots/keys/vals are Memory{T} fields, and
# haskey/getindex/setindex! today dispatch into pure-Julia methods in
# base/dict.jl (linear probing over Memory arrays) rather than a Rust builtin
# fast path.

# 1. Counting / histogram loop: d[k] = get(d, k, 0) + 1
function histogram_loop(n::Int64)::Int64
    d = Dict{Int64,Int64}()
    i::Int64 = 0
    while i < n
        k = i % 100
        d[k] = get(d, k, 0) + 1
        i += 1
    end
    return length(d)
end

# 2. Membership-check loop: haskey in a loop (no mutation).
function haskey_loop(n::Int64)::Int64
    d = Dict{Int64,Int64}()
    j::Int64 = 0
    while j < 50
        d[j] = j * j
        j += 1
    end
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        k = i % 100
        if haskey(d, k)
            total += 1
        end
        i += 1
    end
    return total
end

# 3. Lookup-heavy loop: getindex on a pre-populated dict.
function lookup_loop(n::Int64)::Int64
    d = Dict{Int64,Int64}()
    j::Int64 = 0
    while j < 200
        d[j] = j * 2
        j += 1
    end
    total::Int64 = 0
    i::Int64 = 0
    while i < n
        k = i % 200
        total += d[k]
        i += 1
    end
    return total
end

println("histogram_loop=", histogram_loop(2000000))
println("haskey_loop=", haskey_loop(2000000))
println("lookup_loop=", lookup_loop(2000000))
