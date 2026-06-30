using DataStructures
using Base.Order: Forward, Reverse

function data_structures_heap_contract_8141()
    xs = heapify([10, 9, 8, 7, 6, 5, 4])
    popped = Int64[]
    while !isempty(xs)
        push!(popped, heappop!(xs))
    end
    ok_pop = popped == [4, 5, 6, 7, 8, 9, 10]

    ys = Int64[]
    for x in [3, 1, 4, 1, 5]
        heappush!(ys, x)
    end
    ok_push = isheap(ys, Forward) && heappop!(ys) == 1

    zs = [1, 5, 4, 3, 2]
    DataStructures.percolate_down!(zs, 1, 1, Reverse)
    ok_down_reverse = zs == [5, 3, 4, 1, 2]

    ws = [5, 4, 3, 2, 10]
    DataStructures.percolate_up!(ws, 5, 10, Reverse)
    ok_up_reverse = ws == [10, 5, 3, 2, 4]

    rs = heapify!([1, 2, 3], Reverse)
    ok_reverse_heap = isheap(rs, Reverse) && heappop!(rs, Reverse) == 3

    return ok_pop && ok_push && ok_down_reverse && ok_up_reverse && ok_reverse_heap
end

data_structures_heap_contract_8141()
