module JSXGraph

include("types.jl")
include("api.jl")
include("elements.jl")

export Board, JSXElement, JSFunction, View3D
export board, point, line, segment, circle, polygon, text, functiongraph, html
export view3d, curve3d, point3d, line3d, parametricsurface3d

end
