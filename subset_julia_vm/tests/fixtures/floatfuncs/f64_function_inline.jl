function f(x::Float64)::Float64
    x * 2.0 + 1.0
end

function g(x::Float64, y::Float64)::Float64
    f(x) + f(y)
end

function sum_map(n::Int)::Float64
    s = 0.0
    for i in 1:n
        s += f(Float64(i))
    end
    s
end

function count_eq_three(n::Int)::Int
    c = 0
    for i in 1:n
        v = f(Float64(i))
        if v == 3.0
            c += 1
        end
    end
    c
end

function count_neq_three(n::Int)::Int
    c = 0
    for i in 1:n
        v = f(Float64(i))
        if v != 3.0
            c += 1
        end
    end
    c
end

@assert g(1.0, 2.0) == 8.0
@assert sum_map(10) == 120.0
@assert count_eq_three(10) == 1
@assert count_neq_three(10) == 9
true
