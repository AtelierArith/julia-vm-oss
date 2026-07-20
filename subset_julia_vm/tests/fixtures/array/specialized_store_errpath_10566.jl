# Issue #10566: stores that PRECEDE a mid-loop error are observable under
# try/catch (upstream semantics). A BoundsError raised partway through an
# array-store loop must leave the already-written elements in place.
function overrun!(a, n)
    s = 0.0
    for i in 1:n
        a[i] = i
        s += a[i]
    end
    return s
end

a = zeros(3)
s = 0.0
try
    global s = overrun!(a, 10)
catch e
    # BoundsError on i == 4
end
@assert a == [1.0, 2.0, 3.0]

# Direct top-level form (the shape verified on main).
b = zeros(3)
t = 0.0
try
    for i in 1:10
        b[i] = i
        global t += b[i]
    end
catch
end
@assert t == 6.0
@assert b == [1.0, 2.0, 3.0]

true
