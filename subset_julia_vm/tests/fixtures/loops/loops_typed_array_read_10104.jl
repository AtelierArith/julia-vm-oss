# Issue #10104: typed-loop recognizer covers read-only 1-D array reductions
# (`for i in 1:length(x); s += x[i]; end`). The typed fast path must be
# byte-identical to the interpreter for every shape below, including empty
# arrays, boundary values, early break/return, and an out-of-bounds access that
# must still raise a catchable BoundsError.

function fsum(x::Vector{Float64})
    s = 0.0
    for i in 1:length(x)
        s += x[i]
    end
    s
end

function isum(x::Vector{Int64})
    s = 0
    for i in 1:length(x)
        s += x[i]
    end
    s
end

function dotp(x::Vector{Float64}, y::Vector{Float64})
    s = 0.0
    for i in 1:length(x)
        s += x[i] * y[i]
    end
    s
end

function imax(x::Vector{Int64})
    m = x[1]
    for i in 2:length(x)
        if x[i] > m
            m = x[i]
        end
    end
    m
end

function firstover(x::Vector{Float64}, t::Float64)
    r = 0.0
    for i in 1:length(x)
        if x[i] > t
            r = x[i]
            break
        end
    end
    r
end

function findv(x::Vector{Int64}, v::Int64)
    for i in 1:length(x)
        if x[i] == v
            return i
        end
    end
    -1
end

# n may exceed length(x): the read must raise a catchable BoundsError, exactly
# like the interpreter (the typed fast path bails on the out-of-bounds access).
function sum_n(x::Vector{Float64}, n::Int64)
    s = 0.0
    try
        for i in 1:n
            s += x[i]
        end
    catch e
        return -1.0
    end
    s
end

println(fsum(Float64[]) == 0.0)
println(fsum([1.5, 2.5, 3.0]) == 7.0)
println(isum(Int64[]) == 0)
println(isum([1, 2, 3, 4, 5]) == 15)
println(dotp([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]) == 32.0)
println(imax([3, 1, 5, 2, 4]) == 5)
println(firstover([0.5, 0.2, 0.9, 0.7], 0.6) == 0.9)
println(findv([10, 20, 30], 20) == 2)
println(findv([10, 20, 30], 99) == -1)
println(sum_n([1.0, 2.0], 5) == -1.0)
println(sum_n([1.0, 2.0, 3.0], 3) == 6.0)
# large reduction exercises the native block over many iterations
println(fsum(collect(1.0:1000.0)) == 500500.0)
println(isum(collect(1:1000)) == 500500)
"true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n"
