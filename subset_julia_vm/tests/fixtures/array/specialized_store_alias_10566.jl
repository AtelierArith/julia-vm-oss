# Issue #10566: two locals sharing the same array origin (b = a) must observe
# each other's writes -- no snapshot/copy may be substituted for either.
function fillmul2!(a, b, n)
    for i in 1:n
        a[i] = a[i] * 2
        b[i] = b[i] + 1
    end
    return a
end

a = [1, 2, 3]
b = a          # alias: same origin
fillmul2!(a, b, 3)
# Upstream: each iteration doubles then increments the SAME element.
@assert a == [3, 5, 7]
@assert b === a
@assert b == [3, 5, 7]

# Alias observed by an outer binding while an untyped store loop mutates.
function fill_seq!(x, n)
    for i in 1:n
        x[i] = i * 10
    end
    return x
end
p = zeros(Int64, 4)
q = p                     # alias, no reallocation may occur
fill_seq!(p, 4)
@assert q === p
@assert q == [10, 20, 30, 40]

true
