# VM string operation benchmark (Issue #8629, parent #8612).
#
# Exercises the paths where `Value::Str(String)` clones the string body:
#   1. long-string assignment / argument-passing loops
#   2. Dict{String, Int64} insertion and lookup
#   3. join / split / concatenation
#   4. storing strings into arrays
#
# The workload is deterministic and prints one result line per section so the
# runner can compare sjulia output against upstream julia byte-for-byte.

function make_long_string(target_len::Int64)
    s = "abcdefghijklmnop"
    while length(s) < target_len
        s = s * s
    end
    return s
end

# Probe that forces the callee to receive the string by value.
probe_len(s::String) = length(s) % 97

# 1. Assignment / argument-passing loop over a long string.
function assign_pass_loop(s::String, iters::Int64)
    total = 0
    t = s
    for i in 1:iters
        u = t
        total += probe_len(u)
        t = u
    end
    return total
end

# 2. Dict{String, Int64} insertion and lookup.
function dict_insert_lookup(n::Int64)
    d = Dict{String, Int64}()
    for i in 1:n
        d[string("key_", i)] = i
    end
    total = 0
    for i in 1:n
        total += d[string("key_", i)]
    end
    return total + length(d)
end

# 3. join / split / concatenation.
function join_split_concat(n::Int64)
    parts = String[]
    for i in 1:n
        push!(parts, string("part", i))
    end
    joined = join(parts, ",")
    pieces = split(joined, ",")
    total = 0
    for p in pieces
        total += length(p)
    end
    acc = ""
    for i in 1:n
        acc = acc * "x"
    end
    return total + length(acc) + length(joined)
end

# 4. Storing long strings into arrays.
function array_store(s::String, n::Int64)
    arr = String[]
    for i in 1:n
        push!(arr, s)
    end
    total = 0
    for x in arr
        total += probe_len(x)
    end
    return total
end

function main()
    long_s = make_long_string(4096)
    println("len=", length(long_s))
    println("assign_pass=", assign_pass_loop(long_s, 20000))
    println("dict=", dict_insert_lookup(4000))
    println("join_split=", join_split_concat(2000))
    println("array_store=", array_store(long_s, 20000))
end

main()
