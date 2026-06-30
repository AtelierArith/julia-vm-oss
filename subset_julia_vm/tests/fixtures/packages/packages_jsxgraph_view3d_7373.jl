using JSXGraph

b = board(; xlim=(-5, 5), ylim=(-5, 5)) do board_ref
    v = view3d([-4.0, -3.0], [8.0, 8.0],
               Any[Any[-2.0, 2.0], Any[-2.0, 2.0], Any[-2.0, 2.0]];
               xPlaneRear=true)
    p = point3d(0.0, 0.0, 0.0; name="O")
    q = point3d(1.0, 1.0, 1.0; name="Q")
    l = line3d(p, q)
    c = curve3d("2*Math.sin(3*t)", "2*Math.sin(4*t)", "2*Math.sin(5*t)",
                [0.0, 6.283185307179586])
    push!(v, p, q, l, c)
    push!(board_ref, v)
end

v = b.elements[1]
length(b.elements) == 1 &&
    length(v.elements) == 4 &&
    v.elements[1].type_name == :point3d &&
    v.elements[3].type_name == :line3d &&
    v.elements[4].type_name == :curve3d
