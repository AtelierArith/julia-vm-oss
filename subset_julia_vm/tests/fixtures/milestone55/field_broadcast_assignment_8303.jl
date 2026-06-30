struct Holder8303
    xs
end

function bump8303(h, ys)
    h.xs .+= ys
    return h.xs
end

function fill8303(h, ys)
    h.xs .= ys
    return h.xs
end

bumped = bump8303(Holder8303([1, 2]), [3, 4])
filled = fill8303(Holder8303([0, 0]), [5, 6])
bumped[2] == 6 && filled[2] == 6
