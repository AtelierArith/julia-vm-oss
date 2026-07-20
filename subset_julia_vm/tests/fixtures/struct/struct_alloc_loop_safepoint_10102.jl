# Struct-allocating loop safepoint regression (Issue #10102).
#
# The memory-waterline safepoint (which triggers struct-heap compaction when the
# soft limit is exceeded) was moved off the per-instruction dispatch path onto
# loop back-edges + Call/Return boundaries. A loop that allocates a fresh struct
# every iteration must still produce correct results: compaction may relocate
# live struct storage at the back-edge, so any live reference held across the
# back-edge (accumulator fields, arrays of structs) must survive relocation.

struct Point
    x::Int
    y::Int
end

# Accumulate over many freshly-allocated structs. Each iteration allocates a new
# Point (a Call boundary) and the loop header is a back-edge; both are safepoints
# now. The running sum is a live value carried across every back-edge.
function sum_points(n)
    sx = 0
    sy = 0
    for i in 1:n
        p = Point(i, 2 * i)
        sx += p.x
        sy += p.y
    end
    return (sx, sy)
end

# Keep an array of live structs alive across the whole loop, so a mid-loop
# compaction must relocate storage that is still referenced afterwards.
function collect_points(n)
    pts = Point[]
    for i in 1:n
        push!(pts, Point(i, i * i))
    end
    total = 0
    for p in pts
        total += p.x + p.y
    end
    return total
end

# Straight-line-ish allocation via nested calls (exercises the Call/Return
# safepoint even when the back-edge is sparse).
function make_pair(i)
    a = Point(i, i)
    b = Point(a.x + 1, a.y + 1)
    return b.x + b.y
end

function pair_sum(n)
    s = 0
    for i in 1:n
        s += make_pair(i)
    end
    return s
end

let n = 5000
    (sx, sy) = sum_points(n)
    # sum_{i=1}^{n} i = n(n+1)/2 ; sy = 2 * sx
    expected_sx = div(n * (n + 1), 2)
    println(sx == expected_sx)
    println(sy == 2 * expected_sx)

    # collect_points: sum_{i=1}^{n} (i + i^2)
    ct = collect_points(n)
    expected_ct = div(n * (n + 1), 2) + div(n * (n + 1) * (2 * n + 1), 6)
    println(ct == expected_ct)

    # pair_sum: sum_{i=1}^{n} ((i+1) + (i+1)) = 2 * sum_{i=1}^{n} (i+1)
    ps = pair_sum(n)
    expected_ps = 2 * (div(n * (n + 1), 2) + n)
    println(ps == expected_ps)
end

true
