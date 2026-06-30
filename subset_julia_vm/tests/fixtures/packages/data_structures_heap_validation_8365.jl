using DataStructures
using Base.Order: Forward, Reverse

function pop_all_8365(xs, ordering)
    popped = Int64[]
    while !isempty(xs)
        push!(popped, heappop!(xs, ordering))
    end
    return popped
end

function data_structures_heap_validation_8365()
    source = [7, 2, 6, 1, 5, 3, 4]
    copied = heapify(source, Forward)
    ok_heapify_copy = source == [7, 2, 6, 1, 5, 3, 4] &&
        isheap(copied, Forward) &&
        pop_all_8365(copied, Forward) == [1, 2, 3, 4, 5, 6, 7]

    forward_heap = [7, 2, 6, 1, 5, 3, 4]
    heapify!(forward_heap, Forward)
    ok_heapify_bang_forward = isheap(forward_heap, Forward) &&
        pop_all_8365(forward_heap, Forward) == [1, 2, 3, 4, 5, 6, 7]

    reverse_heap = [7, 2, 6, 1, 5, 3, 4]
    heapify!(reverse_heap, Reverse)
    ok_heapify_bang_reverse = isheap(reverse_heap, Reverse) &&
        pop_all_8365(reverse_heap, Reverse) == [7, 6, 5, 4, 3, 2, 1]

    pushed_forward = Int64[]
    for x in [3, 1, 4, 1, 5]
        heappush!(pushed_forward, x, Forward)
    end
    ok_heappush_forward = isheap(pushed_forward, Forward) &&
        pop_all_8365(pushed_forward, Forward) == [1, 1, 3, 4, 5]

    pushed_reverse = Int64[]
    for x in [3, 1, 4, 1, 5]
        heappush!(pushed_reverse, x, Reverse)
    end
    ok_heappush_reverse = isheap(pushed_reverse, Reverse) &&
        pop_all_8365(pushed_reverse, Reverse) == [5, 4, 3, 1, 1]

    down_forward = [1, 4, 2, 7, 6, 3, 5]
    DataStructures.percolate_down!(down_forward, 1, 8, Forward)
    ok_percolate_down_forward =
        down_forward == [2, 4, 3, 7, 6, 8, 5] &&
        isheap(down_forward, Forward)

    down_reverse = [9, 7, 8, 1, 2, 3, 4]
    DataStructures.percolate_down!(down_reverse, 1, 0, Reverse)
    ok_percolate_down_reverse =
        down_reverse == [8, 7, 4, 1, 2, 3, 0] &&
        isheap(down_reverse, Reverse)

    up_forward = [1, 3, 2, 7, 6, 5, 4, 0]
    DataStructures.percolate_up!(up_forward, 8, 0, Forward)
    ok_percolate_up_forward =
        up_forward == [0, 1, 2, 3, 6, 5, 4, 7] &&
        isheap(up_forward, Forward)

    up_reverse = [10, 8, 9, 3, 4, 5, 6, 1, 12]
    DataStructures.percolate_up!(up_reverse, 9, 12, Reverse)
    ok_percolate_up_reverse =
        up_reverse == [12, 10, 9, 8, 4, 5, 6, 1, 3] &&
        isheap(up_reverse, Reverse)

    partial = [100, 80, 90, 10, 70, 30, 40, 5]
    DataStructures.percolate_down!(partial, 1, 60, Reverse, 7)
    ok_partial_reverse =
        partial == [90, 80, 60, 10, 70, 30, 40, 5] &&
        partial[8] == 5 &&
        isheap(partial[1:7], Reverse)

    not_heap_forward = !isheap([2, 1, 3], Forward)
    not_heap_reverse = !isheap([2, 3, 1], Reverse)

    return ok_heapify_copy &&
        ok_heapify_bang_forward &&
        ok_heapify_bang_reverse &&
        ok_heappush_forward &&
        ok_heappush_reverse &&
        ok_percolate_down_forward &&
        ok_percolate_down_reverse &&
        ok_percolate_up_forward &&
        ok_percolate_up_reverse &&
        ok_partial_reverse &&
        not_heap_forward &&
        not_heap_reverse
end

data_structures_heap_validation_8365()
