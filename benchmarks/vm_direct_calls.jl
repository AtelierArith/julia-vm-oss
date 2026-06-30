function tiny_step(x::Float64, i::Int64)::Float64
    x + Float64(i) * 0.25
end

function direct_call_loop(n::Int64)::Int64
    i = 0
    acc = 0.0
    while i < n
        acc = tiny_step(acc, i)
        i = i + 1
    end
    Int64(acc)
end

function main()
    println(direct_call_loop(50000))
end

main()
