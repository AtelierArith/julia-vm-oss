# VM local slot load/store microbenchmark (Issue #4304).
# This keeps the workload scalar and allocation-light so the VM profile focuses
# on repeated Int64/Float64/Bool local bindings in tight loops.

function local_slot_count(n::Int64)::Int64
    i = 0
    total = 0
    acc = 0.0
    flag = true
    while i < n
        x = Float64(i)
        acc = acc + x * 0.5
        if flag
            total = total + i
        else
            total = total - i
        end
        flag = !flag
        i = i + 1
    end
    total + Int64(acc)
end

function main()
    println(local_slot_count(50000))
end

main()
