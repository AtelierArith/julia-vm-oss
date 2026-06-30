using JSXGraph

# A torus surface: the coordinate maps are functions of two parameters (u, v),
# so each JSFunction parent records var=:u and var2=:v.
s = parametricsurface3d(
    "(2.5 + Math.cos(v)) * Math.cos(u)",
    "(2.5 + Math.cos(v)) * Math.sin(u)",
    "Math.sin(v)",
    [0.0, 6.283185307179586],
    [0.0, 6.283185307179586])

s.type_name == :parametricsurface3d &&
    s.parents[1].code == "(2.5 + Math.cos(v)) * Math.cos(u)" &&
    s.parents[1].var == :u &&
    s.parents[1].var2 == :v &&
    s.parents[3].code == "Math.sin(v)" &&
    s.parents[4] == [0.0, 6.283185307179586] &&
    s.parents[5] == [0.0, 6.283185307179586]
