# Issue #3908: isequal/hash for native Array values are routed through
# ArrayValue logical helpers in vm/builtins_equality.rs. Exercise the
# reshape (shared backing) and Float storage paths so any regression in
# the logical-element accessor surfaces here.

let
    base = [1, 2, 3, 4, 5, 6]
    a = reshape(base, 2, 3)
    b = reshape(copy(base), 2, 3)
    # Same shape and element-by-element equal: isequal must return true.
    if !isequal(a, b)
        return false
    end
    # Different shape (same underlying contents) must compare unequal.
    if isequal(a, reshape(copy(base), 3, 2))
        return false
    end
end

let
    # NaN-aware: isequal(NaN, NaN) is true at the array level too, so two
    # Float vectors with NaN at the same slot must be isequal.
    p = [1.0, NaN, 3.0]
    q = [1.0, NaN, 3.0]
    if !isequal(p, q)
        return false
    end
    # Different bit pattern at one slot must fail.
    if isequal(p, [1.0, 0.0, 3.0])
        return false
    end
end

let
    # hash() of equal arrays must agree so Dict / Set lookups stay stable
    # when the array hashing path is routed through the helper.
    p = [1.0, 2.0, 3.0]
    q = [1.0, 2.0, 3.0]
    if hash(p) != hash(q)
        return false
    end
end

true
