using JSXGraph

c = curve3d("2*Math.sin(3*t)", "2*Math.sin(4*t)", "2*Math.sin(5*t)",
            [0.0, 6.283185307179586])

c.type_name == :curve3d &&
    c.parents[1].code == "2*Math.sin(3*t)" &&
    c.parents[2].code == "2*Math.sin(4*t)" &&
    c.parents[3].code == "2*Math.sin(5*t)" &&
    c.parents[1].var == :t
