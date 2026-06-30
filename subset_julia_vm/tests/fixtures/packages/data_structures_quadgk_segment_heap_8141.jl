using DataStructures
using Base.Order: Reverse

struct QuadGKLikeSegment8141{TX,TI,TE}
    a::TX
    b::TX
    I::TI
    E::TE
end

Base.:(==)(x::QuadGKLikeSegment8141, y::QuadGKLikeSegment8141) =
    x.a == y.a && x.b == y.b && x.I == y.I && x.E == y.E

Base.isless(x::QuadGKLikeSegment8141, y::QuadGKLikeSegment8141) =
    isless(x.E, y.E)

segment8141(id, err) = QuadGKLikeSegment8141(id, id + 1, id * 10, err)
errors8141(segs) = [seg.E for seg in segs]

function data_structures_quadgk_segment_heap_contract_8141()
    segments = [
        segment8141(1, 0.2),
        segment8141(2, 0.9),
        segment8141(3, 0.4),
        segment8141(4, 0.7),
    ]
    heapify!(segments, Reverse)
    ok_heapify = isheap(segments, Reverse) && segments[1].E == 0.9

    popped = heappop!(segments, Reverse)
    ok_pop = popped.E == 0.9 && isheap(segments, Reverse)

    heappush!(segments, segment8141(5, 1.1), Reverse)
    ok_push = isheap(segments, Reverse) && segments[1].E == 1.1

    # Mirrors QuadGK's batched refinement path: replace the root with the
    # element from the bounded active prefix, park the popped segment at the
    # end, then restore heap order only over the active prefix.
    batch_segments = [
        segment8141(1, 0.95),
        segment8141(2, 0.80),
        segment8141(3, 0.70),
        segment8141(4, 0.20),
        segment8141(5, 0.60),
        segment8141(6, 0.30),
    ]
    heapify!(batch_segments, Reverse)
    len = length(batch_segments)
    popped_batch = batch_segments[1]
    replacement = batch_segments[len]
    batch_segments[len] = popped_batch
    DataStructures.percolate_down!(batch_segments, 1, replacement, Reverse, len - 1)

    active = batch_segments[1:(len - 1)]
    parked = batch_segments[len]
    ok_bounded_down =
        parked == popped_batch &&
        parked.E == 0.95 &&
        isheap(active, Reverse) &&
        active[1].E == 0.80 &&
        errors8141(batch_segments) == [0.80, 0.60, 0.70, 0.20, 0.30, 0.95]

    return ok_heapify && ok_pop && ok_push && ok_bounded_down
end

data_structures_quadgk_segment_heap_contract_8141()
