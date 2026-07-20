using DataStructures

struct HeapBox8509
    E::Float64
end

Base.isless(i::HeapBox8509, j::HeapBox8509) = isless(i.E, j.E)

function data_structures_binary_max_heap_contract_8509()
    h = BinaryMaxHeap{Int64}()
    ok_empty = length(h) == 0 && isempty(h)

    push!(h, 2)
    push!(h, 5)
    push!(h, 1)

    ok_order = first(h) == 5 &&
        pop!(h) == 5 &&
        pop!(h) == 2 &&
        pop!(h) == 1 &&
        isempty(h)

    boxes = DataStructures.BinaryMaxHeap{HeapBox8509}()
    push!(boxes, HeapBox8509(0.2))
    push!(boxes, HeapBox8509(0.8))
    push!(boxes, HeapBox8509(0.4))
    ok_box_order = pop!(boxes).E == 0.8 &&
        pop!(boxes).E == 0.4 &&
        pop!(boxes).E == 0.2 &&
        length(boxes) == 0

    push!(boxes, HeapBox8509(0.1))
    empty!(boxes)
    ok_empty_bang = length(boxes) == 0 && length(boxes.valtree) == 0

    seeded = DataStructures.BinaryMaxHeap([HeapBox8509(0.3), HeapBox8509(0.9), HeapBox8509(0.6)])
    ok_seeded = first(seeded).E == 0.9 && length(seeded.valtree) == 3

    sample = HeapBox8509(1.4)
    typed_boxes = DataStructures.BinaryMaxHeap{typeof(sample)}()
    push!(typed_boxes, sample)
    push!(typed_boxes, HeapBox8509(1.1))
    ok_typeof_arg = pop!(typed_boxes).E == 1.4 && pop!(typed_boxes).E == 1.1

    return ok_empty &&
           ok_order &&
           ok_box_order &&
           ok_empty_bang &&
           ok_seeded &&
           ok_typeof_arg
end

data_structures_binary_max_heap_contract_8509()
