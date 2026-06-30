module Plots

using SciMLBase

include("types.jl")
include("api.jl")

export plot, plot!, plot3d, plot3d!, scatter, scatter!, bar, bar!, histogram, histogram!, weights, heatmap, heatmap!, surface, Plot, Series
export Animation, AnimatedGif, current, frame, gif, @animate, @gif
export title!, xlims!, xlims, ylims!, ylims, hline!, hline, vline!, vline

end
