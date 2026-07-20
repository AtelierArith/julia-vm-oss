# vm_aot equivalence corpus widening (Issue #10815): `break`/`continue`
# inside a `while true` loop, under the AoT minimal-prelude codegen path.
function count_evens(n::Int64)::Int64
    count = 0
    i = 0
    while true
        i = i + 1
        if i > n
            break
        end
        if i % 2 != 0
            continue
        end
        count = count + 1
    end
    count
end

println(count_evens(20))

count_evens(20) == 10
