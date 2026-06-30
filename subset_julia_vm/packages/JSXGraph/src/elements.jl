function _create_element(type_name::Symbol, parents; kwargs...)
    return JSXElement(_new_id(), type_name, parents, _kw_to_attrs(kwargs))
end

point(x, y; kwargs...) = _create_element(:point, Any[x, y]; kwargs...)
line(a, b; kwargs...) = _create_element(:line, Any[a, b]; kwargs...)
segment(a, b; kwargs...) = _create_element(:segment, Any[a, b]; kwargs...)
circle(center, r; kwargs...) = _create_element(:circle, Any[center, r]; kwargs...)
polygon(pts...; kwargs...) = _create_element(:polygon, Any[pts...]; kwargs...)
text(x, y, s; kwargs...) = _create_element(:text, Any[x, y, s]; kwargs...)

function curve3d(fx, fy, fz, range; kwargs...)
    return _create_element(:curve3d, Any[_jsf(fx), _jsf(fy), _jsf(fz), range]; kwargs...)
end

point3d(x, y, z; kwargs...) = _create_element(:point3d, Any[x, y, z]; kwargs...)
line3d(a, b; kwargs...) = _create_element(:line3d, Any[a, b]; kwargs...)

# Parametric surface FX(u,v), FY(u,v), FZ(u,v) over the rectangles `urange` and
# `vrange` (each a 2-element `[lo, hi]`). The coordinate expressions are raw
# JavaScript strings in `u` and `v`, carried as two-argument JSFunction values.
function parametricsurface3d(fx, fy, fz, urange, vrange; kwargs...)
    return _create_element(:parametricsurface3d,
        Any[_jsf2(fx), _jsf2(fy), _jsf2(fz), urange, vrange]; kwargs...)
end

function functiongraph(f; a=-5, b=5, n=100, kwargs...)
    if n <= 1
        error("functiongraph requires n >= 2")
    end
    step = (b - a) / (n - 1)
    xs = Float64[]
    ys = Float64[]
    i = 0
    while i < n
        x = a + i * step
        push!(xs, x)
        push!(ys, f(x))
        i += 1
    end
    return _create_element(:curve, Any[xs, ys]; kwargs...)
end
