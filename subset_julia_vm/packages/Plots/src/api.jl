function _plots_check_aspect_ratio(ar)
    if isa(ar, Symbol)
        if ar === :auto || ar === :none || ar === :equal
            return ar
        end
        throw(ArgumentError("Invalid `aspect_ratio` = $ar"))
    elseif ar === true
        return 1
    elseif ar === false
        return 0
    elseif isa(ar, Number)
        return ar
    end
    throw(ArgumentError("Invalid `aspect_ratio` = $ar"))
end

function _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    ar = aspect_ratio
    if aspectratio !== nothing
        ar = aspectratio
    end
    if axis_ratio !== nothing
        ar = axis_ratio
    end
    if axisratio !== nothing
        ar = axisratio
    end
    if ratio !== nothing
        ar = ratio
    end
    return _plots_check_aspect_ratio(ar)
end

function _plots_current_aspect_ratio()
    if length(_CURRENT_ASPECT_RATIO) == 0
        return :auto
    end
    return _CURRENT_ASPECT_RATIO[1]
end

# `_CURRENT_TITLE` tracks the most-recently-created plot's title so a bare
# `current()` / `frame` (and hence `@gif`/`@animate`) snapshots it (Issue #7030).
function _plots_current_title()
    if length(_CURRENT_TITLE) == 0
        return ""
    end
    return _CURRENT_TITLE[1]
end

function _plots_set_current!(series, aspect_ratio, title, xlims, ylims, hlines, vlines)
    if length(_CURRENT_SERIES) == 0
        push!(_CURRENT_SERIES, series)
    else
        _CURRENT_SERIES[1] = series
    end
    if length(_CURRENT_ASPECT_RATIO) == 0
        push!(_CURRENT_ASPECT_RATIO, aspect_ratio)
    else
        _CURRENT_ASPECT_RATIO[1] = aspect_ratio
    end
    if length(_CURRENT_TITLE) == 0
        push!(_CURRENT_TITLE, title)
    else
        _CURRENT_TITLE[1] = title
    end
    if length(_CURRENT_XLIMS) == 0
        push!(_CURRENT_XLIMS, xlims)
    else
        _CURRENT_XLIMS[1] = xlims
    end
    if length(_CURRENT_YLIMS) == 0
        push!(_CURRENT_YLIMS, ylims)
    else
        _CURRENT_YLIMS[1] = ylims
    end
    if length(_CURRENT_HLINES) == 0
        push!(_CURRENT_HLINES, hlines)
    else
        _CURRENT_HLINES[1] = hlines
    end
    if length(_CURRENT_VLINES) == 0
        push!(_CURRENT_VLINES, vlines)
    else
        _CURRENT_VLINES[1] = vlines
    end
    return nothing
end

function _plots_current_xlims()
    length(_CURRENT_XLIMS) == 0 ? nothing : _CURRENT_XLIMS[1]
end

function _plots_current_ylims()
    length(_CURRENT_YLIMS) == 0 ? nothing : _CURRENT_YLIMS[1]
end

function _plots_current_hlines()
    length(_CURRENT_HLINES) == 0 ? Float64[] : _CURRENT_HLINES[1]
end

function _plots_current_vlines()
    length(_CURRENT_VLINES) == 0 ? Float64[] : _CURRENT_VLINES[1]
end

function _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    if aspect_ratio === nothing &&
            aspectratio === nothing &&
            axis_ratio === nothing &&
            axisratio === nothing &&
            ratio === nothing
        return _plots_current_aspect_ratio()
    end
    return _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
end

function plot(f::Function; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    # Match upstream Plots.jl: when no xlims is given, `plot(f::Function)` falls
    # back to xmin = -5, xmax = 5 via PlotUtils.tryrange before delegating to
    # `_scaled_adapted_grid` (extern/Plots.jl/RecipesPipeline/src/user_recipe.jl:219).
    # We approximate the adaptive grid with uniform sampling at 100 points,
    # which is dense enough for the small REPL canvas.
    n = 100
    xmin = -5.0
    xmax = 5.0
    step = (xmax - xmin) / (n - 1)
    xs = Float64[]
    ys = Float64[]
    i = 0
    while i < n
        x = xmin + i * step
        push!(xs, x)
        push!(ys, f(x))
        i += 1
    end
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(xs, ys, :line, ar, title, label, xlims, ylims)
end

# `plot(y::Vector)` uses indices as x; `plot(y::Number)` seeds a one-point plot
# (the Issue #6355 animation MVP `plot(1)`); `plot(p::Plot)` re-selects an existing
# plot (Issue #7026). With #7021 fixed (a 3rd single-arg overload no longer drops a
# sibling's kwarg) these are separate typed methods; each carries `title=` (#7030).
function plot(y::Vector; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    xs = collect(1:length(y))
    return plot(xs, y; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
end

function plot(y::Number; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(Float64[1.0], Float64[float(y)], :line, ar, title, label, xlims, ylims)
end

function _plots_existing_plot_aspect_ratio(p::Plot, aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    if aspect_ratio === nothing &&
            aspectratio === nothing &&
            axis_ratio === nothing &&
            axisratio === nothing &&
            ratio === nothing
        return p.aspect_ratio
    end
    ar = aspect_ratio === nothing ? :auto : aspect_ratio
    return _plots_aspect_ratio_kw(ar, aspectratio, axis_ratio, axisratio, ratio)
end

# `plot(p::Plot)` re-selects an existing Plot as the current plot (Issue #7026),
# carrying its title/xlims/ylims/hlines/vlines forward unless a new one is given
# (Issue #7030, #7850). It snapshots the series rather than sharing `p.series`,
# matching upstream copy semantics and keeping later `plot!` calls from mutating
# the source plot (Issue #7149).
function plot(p::Plot; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_existing_plot_aspect_ratio(p, aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    t = isempty(title) ? p.title : title
    series = _plots_copy_series_list(p.series)
    _plots_set_current!(series, ar, t, p.xlims, p.ylims, p.hlines, p.vlines)
    return Plot(series, p.backend, ar, t, p.xlims, p.ylims, p.hlines, p.vlines)
end

function plot(f::Function, x; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    return plot(x, f; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
end

function plot(x, f::Function; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    xs = collect(x)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(xs, map(f, xs), :line, ar, title, label, xlims, ylims)
end

function plot(x, y; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(x, y, :line, ar, title, label, xlims, ylims)
end

function _ode_component_count(sol::SciMLBase.ODESolution)
    if length(sol.u) == 0
        return 0
    end
    u = sol.u[1]
    if u isa AbstractVector
        return length(u)
    end
    return 1
end

function _ode_component_value(u, idx)
    if u isa AbstractVector
        return u[idx]
    elseif idx == 1
        return u
    end
    error("scalar ODESolution only has component 1")
end

function _ode_component_values(sol::SciMLBase.ODESolution, idx)
    ys = Any[]
    for u in sol.u
        push!(ys, _ode_component_value(u, idx))
    end
    return ys
end

# ── ODESolution plot recipe pipeline (Issue #7987) ──────────────────────────
# Upstream Plots realizes `plot(sol)` through the RecipesBase `@recipe` that
# SciMLBase defines on `AbstractODESolution`. Mirror that with a minimal recipe
# mechanism instead of a hard-coded `plot(sol)` special case: `apply_recipe(obj;
# attrs...)` IS the recipe — it returns the list of `Series` (plus a 3D hint) for
# the object — and `plot`/`plot!` apply it and assemble the artifact. Attributes
# flow through the recipe: `idxs` (component selection / phase path), `vars`
# (upstream's deprecated alias for `idxs`), and `denseplot` / `plotdensity`
# (sample the callable solution `sol(t)`, #7982, on a fine grid for a smoother
# curve). The recipe is registered on `AbstractODESolution`; the concrete `plot`
# entry keeps a `::ODESolution` annotation for reliable dispatch (sjulia abstract
# annotations can mis-match across modules — StatsPlots dispatch note).
#
# `_has_plot_recipe(obj)` is the registry hook other types can extend.
_has_plot_recipe(obj) = false
_has_plot_recipe(sol::SciMLBase.AbstractODESolution) = true

# Sample grid for the recipe: the saved times, or a `plotdensity`-point fine grid
# spanning the solution interval when `denseplot=true`.
function _ode_recipe_times(sol, denseplot, plotdensity)
    if !denseplot
        return (_plots_copy_data(sol.t), false)
    end
    t0 = sol.t[1]
    t1 = sol.t[end]
    n = plotdensity < 2 ? 2 : plotdensity
    ts = Float64[]
    i = 0
    while i < n
        push!(ts, t0 + (t1 - t0) * i / (n - 1))
        i += 1
    end
    return (ts, true)
end

# Component `idx` over the sample grid: the saved states, or `sol(t)` samples.
function _ode_recipe_component(sol, idx, ts, dense)
    if !dense
        return _ode_component_values(sol, idx)
    end
    ys = Any[]
    for t in ts
        push!(ys, _ode_component_value(sol(t), idx))
    end
    return ys
end

# The ODESolution recipe: returns `(series::Vector, is3d::Bool)`.
function apply_recipe(sol::SciMLBase.AbstractODESolution; idxs=nothing, vars=nothing,
                      denseplot=false, plotdensity=100, label=nothing)
    idxs = idxs === nothing ? vars : idxs
    ts, dense = _ode_recipe_times(sol, denseplot, plotdensity)
    if idxs === nothing
        series = Any[]
        n = _ode_component_count(sol)
        i = 1
        while i <= n
            push!(series, Series(ts, _ode_recipe_component(sol, i, ts, dense), label, :line))
            i += 1
        end
        return (series, false)
    elseif idxs isa Tuple
        n = length(idxs)
        if n == 1
            return (Any[Series(ts, _ode_recipe_component(sol, idxs[1], ts, dense), label, :line)], false)
        elseif n == 2
            return (Any[Series(_ode_recipe_component(sol, idxs[1], ts, dense), _ode_recipe_component(sol, idxs[2], ts, dense), label, :line)], false)
        elseif n == 3
            return (Any[Series(_ode_recipe_component(sol, idxs[1], ts, dense), _ode_recipe_component(sol, idxs[2], ts, dense), _ode_recipe_component(sol, idxs[3], ts, dense), label, :path3d)], true)
        end
        error("ODESolution idxs must contain 1, 2, or 3 components")
    end
    return (Any[Series(ts, _ode_recipe_component(sol, idxs, ts, dense), label, :line)], false)
end

# Apply the recipe and assemble the Plot (the pipeline). Produces the same
# artifact shape as the former hard-coded conversion (no-regression gate).
function plot(sol::SciMLBase.ODESolution; idxs=nothing, vars=nothing, denseplot=false,
              plotdensity=100, aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing,
              axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    series, is3d = apply_recipe(sol; idxs=idxs, vars=vars, denseplot=denseplot,
                                plotdensity=plotdensity, label=label)
    _plots_set_current!(series, ar, title, nothing, nothing, Float64[], Float64[])
    return Plot(series, :text, ar, title)
end

# `plot!(sol)` overlays the recipe series onto the current plot.
function plot!(sol::SciMLBase.ODESolution; idxs=nothing, vars=nothing, denseplot=false,
               plotdensity=100, aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing,
               axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    series, is3d = apply_recipe(sol; idxs=idxs, vars=vars, denseplot=denseplot,
                                plotdensity=plotdensity, label=label)
    if is3d
        last = nothing
        for s in series
            last = _append_to_current_3d(s.x, s.y, s.z, s.seriestype, aspect_ratio, title, s.label)
        end
        return last
    end
    last = nothing
    for s in series
        last = _append_to_current(s.x, s.y, s.seriestype, aspect_ratio, title, s.label)
    end
    return last
end

_plots_copy_data(v) = collect(v)

function _new_plot(x, y, seriestype::Symbol, aspect_ratio, title, label, xlims, ylims)
    s = Series(_plots_copy_data(x), _plots_copy_data(y), label, seriestype)
    series = [s]
    _plots_set_current!(series, aspect_ratio, title, xlims, ylims, Float64[], Float64[])
    return Plot(series, :text, aspect_ratio, title, xlims, ylims)
end
function _new_plot(x, y, seriestype::Symbol, aspect_ratio, title, label)
    return _new_plot(x, y, seriestype, aspect_ratio, title, label, nothing, nothing)
end
_new_plot(x, y, seriestype::Symbol, aspect_ratio, title) = _new_plot(x, y, seriestype, aspect_ratio, title, nothing)
_new_plot(x, y, seriestype::Symbol, aspect_ratio) = _new_plot(x, y, seriestype, aspect_ratio, "", nothing)
_new_plot(x, y, seriestype::Symbol) = _new_plot(x, y, seriestype, :auto, "", nothing)

function _new_plot_3d(x, y, z, seriestype::Symbol, aspect_ratio, title, label, levels)
    s = Series(_plots_copy_data(x), _plots_copy_data(y), _plots_copy_data(z), label, seriestype, levels)
    series = [s]
    _plots_set_current!(series, aspect_ratio, title, nothing, nothing, Float64[], Float64[])
    return Plot(series, :text, aspect_ratio, title)
end
function _new_plot_3d(x, y, z, seriestype::Symbol, aspect_ratio, title, label)
    return _new_plot_3d(x, y, z, seriestype, aspect_ratio, title, label, nothing)
end
_new_plot_3d(x, y, z, seriestype::Symbol, aspect_ratio, title) = _new_plot_3d(x, y, z, seriestype, aspect_ratio, title, nothing)
_new_plot_3d(x, y, z, seriestype::Symbol, aspect_ratio) = _new_plot_3d(x, y, z, seriestype, aspect_ratio, "", nothing)
_new_plot_3d(x, y, z, seriestype::Symbol) = _new_plot_3d(x, y, z, seriestype, :auto, "", nothing)

# `plot!`/`scatter!` keep the current title/xlims/ylims/hlines/vlines unless the
# caller passes a new one (empty title means "unspecified"; missing axis settings
# are carried forward from the current-plot registry).
function _append_to_current(x, y, seriestype::Symbol, aspect_ratio, title, label)
    if length(_CURRENT_SERIES) == 0
        ar = aspect_ratio === nothing ? :auto : aspect_ratio
        return _new_plot(x, y, seriestype, ar, title, label)
    end
    current_series = _CURRENT_SERIES[1]
    s = Series(x, y, label, seriestype)
    push!(current_series, s)
    ar = aspect_ratio === nothing ? _plots_current_aspect_ratio() : aspect_ratio
    t = isempty(title) ? _plots_current_title() : title
    xl = _plots_current_xlims()
    yl = _plots_current_ylims()
    hl = _plots_current_hlines()
    vl = _plots_current_vlines()
    _plots_set_current!(current_series, ar, t, xl, yl, hl, vl)
    return Plot(current_series, :text, ar, t, xl, yl, hl, vl)
end
_append_to_current(x, y, seriestype::Symbol, aspect_ratio, title) = _append_to_current(x, y, seriestype, aspect_ratio, title, nothing)
_append_to_current(x, y, seriestype::Symbol, aspect_ratio) = _append_to_current(x, y, seriestype, aspect_ratio, "", nothing)
_append_to_current(x, y, seriestype::Symbol) = _append_to_current(x, y, seriestype, nothing, "", nothing)

function _append_to_current_3d(x, y, z, seriestype::Symbol, aspect_ratio, title, label, levels)
    if length(_CURRENT_SERIES) == 0
        ar = aspect_ratio === nothing ? :auto : aspect_ratio
        return _new_plot_3d(x, y, z, seriestype, ar, title, label, levels)
    end
    current_series = _CURRENT_SERIES[1]
    s = Series(x, y, z, label, seriestype, levels)
    push!(current_series, s)
    ar = aspect_ratio === nothing ? _plots_current_aspect_ratio() : aspect_ratio
    t = isempty(title) ? _plots_current_title() : title
    xl = _plots_current_xlims()
    yl = _plots_current_ylims()
    hl = _plots_current_hlines()
    vl = _plots_current_vlines()
    _plots_set_current!(current_series, ar, t, xl, yl, hl, vl)
    return Plot(current_series, :text, ar, t, xl, yl, hl, vl)
end
function _append_to_current_3d(x, y, z, seriestype::Symbol, aspect_ratio, title, label)
    return _append_to_current_3d(x, y, z, seriestype, aspect_ratio, title, label, nothing)
end
_append_to_current_3d(x, y, z, seriestype::Symbol, aspect_ratio, title) = _append_to_current_3d(x, y, z, seriestype, aspect_ratio, title, nothing)
_append_to_current_3d(x, y, z, seriestype::Symbol, aspect_ratio) = _append_to_current_3d(x, y, z, seriestype, aspect_ratio, "", nothing)
_append_to_current_3d(x, y, z, seriestype::Symbol) = _append_to_current_3d(x, y, z, seriestype, nothing, "", nothing)

function plot!(f::Function; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    n = 100
    xmin = -5.0
    xmax = 5.0
    step = (xmax - xmin) / (n - 1)
    xs = Float64[]
    ys = Float64[]
    i = 0
    while i < n
        x = xmin + i * step
        push!(xs, x)
        push!(ys, f(x))
        i += 1
    end
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(xs, ys, :line, ar, title, label)
end

function plot!(y::Vector; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    xs = collect(1:length(y))
    return plot!(xs, y; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

function plot!(f::Function, x; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    return plot!(x, f; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

function plot!(x, f::Function; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    xs = collect(x)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(xs, map(f, xs), :line, ar, title, label)
end

function plot!(x, y; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(x, y, :line, ar, title, label)
end

# scatter -- like plot, but renders discrete markers. Mirrors upstream
# Plots.jl's `scatter(args...)` ≡ `plot(args...; seriestype = :scatter)`
# (extern/Plots.jl/PlotsBase/src/shorthands.jl).
function scatter(f::Function; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    n = 100
    xmin = -5.0
    xmax = 5.0
    step = (xmax - xmin) / (n - 1)
    xs = Float64[]
    ys = Float64[]
    i = 0
    while i < n
        x = xmin + i * step
        push!(xs, x)
        push!(ys, f(x))
        i += 1
    end
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(xs, ys, :scatter, ar, title, label, xlims, ylims)
end

function scatter(y::Vector; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    xs = collect(1:length(y))
    return scatter(xs, y; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
end

# `scatter(M::AbstractMatrix)` plots one series per column, matching upstream
# Plots.jl (each column of a matrix is a separate series, sharing the row-index
# x-axis 1:size(M, 1)). The #7275 Interact `@manipulate` sample relies on this
# for `scatter(rand(10, 2))`, which previously raised
# `MethodError: no method matching scatter(::Matrix{Float64})` (Issue #7322).
# Columns are fed through the untyped 2-arg `scatter(x, y)` / `scatter!(x, y)`,
# so no per-column `::Vector` dispatch is required. `::AbstractMatrix` no longer
# loose-matches a `Function` (e.g. `scatter(sin)` correctly reaches
# `scatter(f::Function)`) now that Issue #7334 is fixed.
function scatter(m::AbstractMatrix; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    n, c = size(m)
    xs = collect(1:n)
    p = scatter(xs, m[:, 1]; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
    j = 2
    while j <= c
        p = scatter!(xs, m[:, j]; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=aspectratio, ratio=ratio, title=title, label=label)
        j += 1
    end
    return p
end

function scatter(f::Function, x; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    return scatter(x, f; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
end

function scatter(x, f::Function; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    xs = collect(x)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(xs, map(f, xs), :scatter, ar, title, label, xlims, ylims)
end

function scatter(x, y; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(x, y, :scatter, ar, title, label, xlims, ylims)
end

# scatter! mirrors `plot!` but emits a :scatter Series. Upstream parity:
# `scatter!(args...)` ≡ `plot!(args...; seriestype = :scatter)`.
function scatter!(f::Function; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    n = 100
    xmin = -5.0
    xmax = 5.0
    step = (xmax - xmin) / (n - 1)
    xs = Float64[]
    ys = Float64[]
    i = 0
    while i < n
        x = xmin + i * step
        push!(xs, x)
        push!(ys, f(x))
        i += 1
    end
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(xs, ys, :scatter, ar, title, label)
end

function scatter!(y::Vector; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    xs = collect(1:length(y))
    return scatter!(xs, y; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

# `scatter!(M::AbstractMatrix)` appends one series per column to the current
# plot (Issue #7322), mirroring `scatter(::AbstractMatrix)`. Uses upstream
# Plots' `::AbstractMatrix` now that the function loose-match is fixed (#7334).
function scatter!(m::AbstractMatrix; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    n, c = size(m)
    xs = collect(1:n)
    p = scatter!(xs, m[:, 1]; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
    j = 2
    while j <= c
        p = scatter!(xs, m[:, j]; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
        j += 1
    end
    return p
end

function scatter!(f::Function, x; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    return scatter!(x, f; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

function scatter!(x, f::Function; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    xs = collect(x)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(xs, map(f, xs), :scatter, ar, title, label)
end

function scatter!(x, y; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(x, y, :scatter, ar, title, label)
end

# bar -- upstream Plots.jl shorthand:
# `bar(args...)` ≡ `plot(args...; seriestype = :bar)`.
function _plots_xy_pair_vector(points)
    xs = Any[]
    ys = Any[]
    for point in points
        if isa(point, Pair)
            push!(xs, point.first)
            push!(ys, point.second)
        else
            push!(xs, point[1])
            push!(ys, point[2])
        end
    end
    return xs, ys
end

function _plots_is_xy_pair_vector(points)
    return length(points) > 0 && (isa(points[1], Tuple) || isa(points[1], Pair))
end

function bar(y::Vector; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    if _plots_is_xy_pair_vector(y)
        xs, ys = _plots_xy_pair_vector(y)
        return bar(xs, ys; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
    end
    xs = collect(1:length(y))
    return bar(xs, y; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=aspectratio, ratio=ratio, title=title, label=label, xlims=xlims, ylims=ylims)
end

function bar(x, y; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(x, y, :bar, ar, title, label, xlims, ylims)
end

function bar!(y::Vector; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    if _plots_is_xy_pair_vector(y)
        xs, ys = _plots_xy_pair_vector(y)
        return bar!(xs, ys; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=aspectratio, ratio=ratio, title=title, label=label)
    end
    xs = collect(1:length(y))
    return bar!(xs, y; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=aspectratio, ratio=ratio, title=title, label=label)
end

function bar!(x, y; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(x, y, :bar, ar, title, label)
end

# histogram -- mirrors upstream Plots.jl's shorthand:
# `histogram(args...)` is `plot(args...; seriestype = :histogram)`, whose
# recipe bins x-only data into bar-like series.
function weights(w)
    return w
end

function _plots_hist_norm_mode(normalize)
    if normalize === true
        return :pdf
    elseif normalize === false
        return :none
    end
    return normalize
end

function _plots_histogram_values_and_weights(y, weights)
    if weights !== nothing && length(weights) != length(y)
        error("weights must have the same length as histogram data")
    end

    values = Float64[]
    value_weights = Float64[]
    i = 1
    for v in y
        fv = Float64(v)
        if isfinite(fv)
            push!(values, fv)
            if weights === nothing
                push!(value_weights, 1.0)
            else
                push!(value_weights, Float64(weights[i]))
            end
        end
        i += 1
    end
    return values, value_weights
end

function _plots_quantile_sorted(sorted_values, p)
    n = length(sorted_values)
    if n == 0
        return NaN
    elseif n == 1
        return sorted_values[1]
    end

    h = (n - 1) * p + 1
    lo = floor(Int, h)
    hi = ceil(Int, h)
    if lo == hi
        return sorted_values[lo]
    end
    return sorted_values[lo] + (h - lo) * (sorted_values[hi] - sorted_values[lo])
end

function _plots_std(values)
    n = length(values)
    if n <= 1
        return 0.0
    end

    total = 0.0
    for v in values
        total += v
    end
    mean = total / n

    ss = 0.0
    for v in values
        d = v - mean
        ss += d * d
    end
    return sqrt(ss / (n - 1))
end

function _plots_clamped_ceil_bins(x)
    if !isfinite(x) || x <= 1.0
        return 1
    end
    n = ceil(Int, x)
    return min(n, 10000)
end

function _plots_auto_binning_nbins(values, mode::Symbol)
    if mode === :auto || mode === :wand
        mode = :fd
    end

    n = length(values)
    if n <= 1
        return 1
    end

    lo, hi = extrema(values)
    span = hi - lo
    if span <= 0.0
        return 1
    end

    nd = Float64(n)^(1.0 / 3.0)
    if mode === :sqrt
        return _plots_clamped_ceil_bins(sqrt(n))
    elseif mode === :sturges
        return _plots_clamped_ceil_bins(log2(n) + 1.0)
    elseif mode === :rice
        return _plots_clamped_ceil_bins(2.0 * nd)
    elseif mode === :scott
        sigma = _plots_std(values)
        if sigma <= 0.0
            return 1
        end
        return _plots_clamped_ceil_bins(span / (3.5 * sigma / nd))
    elseif mode === :fd
        sorted_values = sort(values)
        iqr = _plots_quantile_sorted(sorted_values, 0.75) -
              _plots_quantile_sorted(sorted_values, 0.25)
        if iqr <= 0.0
            iqr = 1.0
        end
        return _plots_clamped_ceil_bins(span / (2.0 * iqr / nd))
    end

    error("Unknown auto-binning mode $mode")
end

function _plots_equal_edges(values, nbins)
    if nbins < 1
        error("histogram bins must be positive")
    end

    lo, hi = extrema(values)
    if lo == hi
        lo -= 0.5
        hi += 0.5
    end

    step = (hi - lo) / nbins
    edges = Float64[]
    i = 0
    while i <= nbins
        push!(edges, lo + i * step)
        i += 1
    end
    edges[length(edges)] = hi
    return edges
end

function _plots_edges_from_bins(values, bins)
    if isa(bins, Integer)
        return _plots_equal_edges(values, Int64(bins))
    elseif isa(bins, Symbol)
        return _plots_equal_edges(values, _plots_auto_binning_nbins(values, bins))
    end

    edges = Float64[]
    for edge in bins
        push!(edges, Float64(edge))
    end
    if length(edges) < 2
        error("histogram bins must contain at least two edges")
    end
    return edges
end

function _plots_bin_centers(edges)
    centers = Float64[]
    i = 1
    while i < length(edges)
        push!(centers, (edges[i] + edges[i + 1]) / 2.0)
        i += 1
    end
    return centers
end

function _plots_find_bin(x, edges)
    nbins = length(edges) - 1
    if nbins < 1 || x < edges[1] || x > edges[length(edges)]
        return 0
    end

    i = 1
    while i <= nbins
        if (x >= edges[i] && x < edges[i + 1]) ||
                (i == nbins && x == edges[i + 1])
            return i
        end
        i += 1
    end
    return 0
end

function _plots_normalize_histogram!(counts, edges, normalize)
    mode = _plots_hist_norm_mode(normalize)
    if mode === :none
        return counts
    end

    total = sum(counts)
    if total == 0.0
        return counts
    end

    i = 1
    while i <= length(counts)
        width = edges[i + 1] - edges[i]
        if width <= 0.0
            error("histogram bin edges must be strictly increasing")
        end

        if mode === :probability
            counts[i] = counts[i] / total
        elseif mode === :pdf
            counts[i] = counts[i] / (total * width)
        elseif mode === :density
            counts[i] = counts[i] / width
        else
            error("Unknown histogram normalization mode $mode")
        end
        i += 1
    end
    return counts
end

function _plots_histogram_xy(y; bins=:auto, weights=nothing, normalize=false)
    values, value_weights = _plots_histogram_values_and_weights(y, weights)
    if length(values) == 0
        if isa(bins, Integer) || isa(bins, Symbol)
            return Float64[], Float64[]
        end
        edges = _plots_edges_from_bins(values, bins)
        return _plots_bin_centers(edges), zeros(Float64, length(edges) - 1)
    end

    edges = _plots_edges_from_bins(values, bins)
    counts = zeros(Float64, length(edges) - 1)
    i = 1
    while i <= length(values)
        bin = _plots_find_bin(values[i], edges)
        if bin > 0
            counts[bin] += value_weights[i]
        end
        i += 1
    end

    return _plots_bin_centers(edges), _plots_normalize_histogram!(counts, edges, normalize)
end

function histogram(y; bins=:auto, weights=nothing, normalize=false, aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, xlims=nothing, ylims=nothing)
    xs, ys = _plots_histogram_xy(y; bins=bins, weights=weights, normalize=normalize)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot(xs, ys, :bar, ar, title, label, xlims, ylims)
end

function histogram!(y; bins=:auto, weights=nothing, normalize=false, aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    xs, ys = _plots_histogram_xy(y; bins=bins, weights=weights, normalize=normalize)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current(xs, ys, :bar, ar, title, label)
end

# --- 3D variants ---

function plot(x, y, z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot_3d(x, y, z, :path3d, ar, title, label)
end

# `plot3d` — upstream Plots.jl shorthand (PlotsBase/src/shorthands.jl):
#   plot3d(args...; kw...) = plot(args...; kw..., seriestype = :path3d)
# We mirror the two forms the Lorenz-attractor sample needs:
#   * `plot3d(x, y, z; kw...)` == `plot(x, y, z; seriestype = :path3d, kw...)`
#   * `plot3d(n::Integer; kw...)` initializes a Plot with `n` EMPTY :path3d series,
#     to be filled later with `push!(plt, x, y, z)`.
# The Lorenz sample passes display-only kwargs (xlim/ylim/zlim/legend/marker) that
# the text/Plotly backends don't model yet; they are accepted and ignored, while
# `title` is captured like every other `plot` constructor (Issue #7030).
function _plots_empty_path3d_series(n::Integer)
    series = Any[]
    i = 0
    while i < n
        push!(series, Series(Float64[], Float64[], Float64[], nothing, :path3d))
        i += 1
    end
    return series
end

function plot3d(n::Integer; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    series = _plots_empty_path3d_series(n)
    _plots_set_current!(series, ar, title, nothing, nothing, Float64[], Float64[])
    return Plot(series, :text, ar, title)
end

function plot3d(x, y, z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot_3d(x, y, z, :path3d, ar, title, label)
end

# `plot3d!(x, y, z; kw...)` appends a :path3d series to the current plot, mirroring
# `plot3d!(args...; kw...) = plot!(args...; kw..., seriestype = :path3d)`.
function plot3d!(x, y, z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, kwargs...)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current_3d(x, y, z, :path3d, ar, title, label)
end

function scatter(x, y, z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot_3d(x, y, z, :scatter3d, ar, title, label)
end

# surface(x, y, z::Matrix) -- Plots.jl compatible 3D surface.
# z orientation: size(z) == (length(y), length(x)), i.e. z[iy, ix].
function _plots_surface_z_from_function(x, y, zf::Function)
    return Float64[zf(xi, yi) for yi in y, xi in x]
end

function surface(x, y, zf::Function; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    return surface(x, y, _plots_surface_z_from_function(x, y, zf); aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

function surface(x, y, z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot_3d(x, y, z, :surface, ar, title, label)
end

# heatmap / contour -- Plots.jl-compatible rectangular z-array rendering.
# `heatmap(z)` / `contour(z)` use column and row indices as x/y coordinates;
# `(x, y, z)` preserves explicit axes. The z orientation matches surface:
# row=y, col=x. Contour is the upstream shorthand shape
# `plot(args...; seriestype=:contour)`, with a small `levels` subset forwarded to
# Plotly's contour trace.
function heatmap(z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    xs = collect(1:size(z, 2))
    ys = collect(1:size(z, 1))
    return heatmap(xs, ys, z; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

function heatmap(x, y, z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot_3d(x, y, z, :heatmap, ar, title, label)
end

function _plots_check_contour_levels(levels)
    if levels === nothing
        return levels
    elseif isa(levels, Integer)
        if levels <= 0
            throw(ArgumentError("must pass a positive number of contours to the levels keyword argument"))
        end
        return levels
    elseif isa(levels, AbstractVector)
        if length(levels) < 2
            throw(ArgumentError("must pass at least two contour levels"))
        end
        return levels
    end
    throw(ArgumentError("levels must be an Integer or AbstractVector for contour plots"))
end

function contour(z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, levels=nothing, kwargs...)
    xs = collect(1:size(z, 2))
    ys = collect(1:size(z, 1))
    return contour(xs, ys, z; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, levels=levels)
end

function contour(x, y, zf::Function; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, levels=nothing, kwargs...)
    return contour(x, y, _plots_surface_z_from_function(x, y, zf); aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, levels=levels)
end

function contour(x, y, z; aspect_ratio=:auto, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, levels=nothing, kwargs...)
    ar = _plots_aspect_ratio_kw(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _new_plot_3d(x, y, z, :contour, ar, title, label, _plots_check_contour_levels(levels))
end

function plot!(x, y, z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current_3d(x, y, z, :path3d, ar, title, label)
end

function scatter!(x, y, z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current_3d(x, y, z, :scatter3d, ar, title, label)
end

function heatmap!(z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    xs = collect(1:size(z, 2))
    ys = collect(1:size(z, 1))
    return heatmap!(xs, ys, z; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label)
end

function heatmap!(x, y, z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current_3d(x, y, z, :heatmap, ar, title, label)
end

function contour!(z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, levels=nothing, kwargs...)
    xs = collect(1:size(z, 2))
    ys = collect(1:size(z, 1))
    return contour!(xs, ys, z; aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, levels=levels)
end

function contour!(x, y, zf::Function; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, levels=nothing, kwargs...)
    return contour!(x, y, _plots_surface_z_from_function(x, y, zf); aspect_ratio=aspect_ratio, aspectratio=aspectratio, axis_ratio=axis_ratio, axisratio=axisratio, ratio=ratio, title=title, label=label, levels=levels)
end

function contour!(x, y, z; aspect_ratio=nothing, aspectratio=nothing, axis_ratio=nothing, axisratio=nothing, ratio=nothing, title="", label=nothing, levels=nothing, kwargs...)
    ar = _plots_append_aspect_ratio(aspect_ratio, aspectratio, axis_ratio, axisratio, ratio)
    return _append_to_current_3d(x, y, z, :contour, ar, title, label, _plots_check_contour_levels(levels))
end

# --- Animation (Issue #6355) ---
#
# Mirrors upstream Plots.jl's `@animate` / `@gif` / `Animation` / `frame` / `gif`,
# but accumulates in-memory `Plot` snapshots instead of writing PNGs + FFmpeg
# (see types.jl). The MVP target:
#
#     using Plots
#     p = plot(1)
#     anim = @animate for x = 0:0.1:5
#         push!(p, 1, sin(x))
#     end
#     gif(anim)

# `current()` rebuilds a Plot from the most-recently-created series, so that a
# bare `frame(anim)` (no plot argument) snapshots whatever the last `plot`/`push!`
# produced — matching upstream `frame(anim, plt = current())`. Axis limits and
# reference lines (Issue #7850) are also captured from their registries.
function current()
    series = length(_CURRENT_SERIES) == 0 ? Any[] : _CURRENT_SERIES[1]
    return Plot(series, :text, _plots_current_aspect_ratio(), _plots_current_title(),
                _plots_current_xlims(), _plots_current_ylims(),
                _plots_current_hlines(), _plots_current_vlines())
end

# --- Appending points to a live plot (Issues #6355 / #7271) ---
#
# Upstream Plots.jl keeps the series data in mutable vectors and mutates them via
# `extend_series!`. sjulia's `Series` is an immutable struct, but `plt.series` is a
# mutable `Vector`, so we append by building a replacement `Series` and writing it
# back into `plt.series[i]`. `_new_plot`/`_new_plot_3d` share the series array with
# `_CURRENT_SERIES`, so this remains visible to `current()`.
#
# Three append modes mirror upstream's three `extend_series!` methods:
#   extend_series!(series, yi)            -> y only, x auto-extended by +1
#   extend_series!(series, xi, yi)        -> x and y appended explicitly
#   extend_series!(series, xi, yi, zi)    -> x, y, and z appended explicitly

# `extend_series!(series, yi)` — append y, auto-extend x by +1 (keep them in step).
# Re-publish `plt` as the current plot after an in-place `push!` extension.
#
# `current()` rebuilds a Plot from the global `_CURRENT_*` holders, which are set
# only when a plot is *constructed* (`plot`, `plot3d`, …). A bare
# `push!(plt, x, y, z)` mutates `plt` but not those holders, so `current()` keeps
# returning the plot's state *before* the pushes. That silently breaks
# `@animate`/`@gif`: the macro snapshots `frame(_anim) == frame(_anim, current())`,
# so every captured frame is the empty pre-push plot (Issue #8214 follow-up — the
# Aizawa/Lorenz `plot3d(1)` + `push!` animations rendered as an empty 2D figure
# because all frames came back empty and the 3D `scene` was never detected).
# Upstream Plots extends the current figure in place, so `current()` reflects each
# `push!`; re-syncing here restores that invariant for the sjulia holder model.
function _plots_resync_current!(plt::Plot)
    # `push!(plt, …)` calls this on *every* push so that `current()` (and thus
    # `@animate`/`@gif`) keeps reflecting the mutated figure (Issue #8214). sjulia
    # stores the series vector into `_CURRENT_SERIES[1]` **by reference**, so once
    # `plt` is the current figure `_CURRENT_SERIES[1] === plt.series` and in-place
    # extends are already visible through `current()`. Re-publishing all 7 holders
    # on every push is then O(pushes×7) redundant global writes and the dominant
    # cost of push!-based animations — ~38× the actual data update (Issue #9203).
    # Only (re)publish when `plt` is not already current; the Issue #8214 invariant
    # (pushed plot becomes/stays current) is preserved in both branches.
    if length(_CURRENT_SERIES) == 0 || !(_CURRENT_SERIES[1] === plt.series)
        _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                            plt.xlims, plt.ylims, plt.hlines, plt.vlines)
    end
    return plt
end

function _plots_extend_y!(plt::Plot, i::Integer, y)
    s = plt.series[i]
    push!(s.x, isempty(s.x) ? 1.0 : s.x[length(s.x)] + 1)
    push!(s.y, y)
    return _plots_resync_current!(plt)
end

# `extend_series!(series, xi, yi)` — append x and y explicitly.
function _plots_extend_xy!(plt::Plot, i::Integer, x, y)
    s = plt.series[i]
    push!(s.x, x)
    push!(s.y, y)
    return _plots_resync_current!(plt)
end

# `extend_series!(series, xi, yi, zi)` — append x, y, and z explicitly. The series
# may have started without a z (e.g. a 2D `plot`), so seed an empty z vector.
function _plots_extend_xyz!(plt::Plot, i::Integer, x, y, z)
    s = plt.series[i]
    push!(s.x, x)
    push!(s.y, y)
    if s.z === nothing
        newz = Float64[z]
        plt.series[i] = Series(s.x, s.y, newz, s.label, s.seriestype, s.levels)
    else
        push!(s.z, z)
    end
    return _plots_resync_current!(plt)
end

# `push!(plt, i, y)` extends series i, auto-extending x — upstream
# `push!(plt, i::Integer, args::Real...)` with a single `args` element.
function Base.push!(plt::Plot, i::Integer, y)
    return _plots_extend_y!(plt, i, y)
end

# `push!(plt, i, x, y)` / `push!(plt, i, x, y, z)` extend series i explicitly —
# upstream `push!(plt, i::Integer, args::Real...)` with 2 or 3 `args` elements.
function Base.push!(plt::Plot, i::Integer, x, y)
    return _plots_extend_xy!(plt, i, x, y)
end

function Base.push!(plt::Plot, i::Integer, x, y, z)
    return _plots_extend_xyz!(plt, i, x, y, z)
end

# `push!(plt, y)` is shorthand for series 1 (upstream: `push!(plt, args::Real...) =
# push!(plt, 1, args...)`).
function Base.push!(plt::Plot, y::Number)
    return _plots_extend_y!(plt, 1, y)
end

# `push!(plt, x, y)` appends a 2D point to series 1; `push!(plt, x, y, z)` appends a
# 3D point. These mirror upstream's `push!(plt, args::Real...)` -> series 1 (#7271).
function Base.push!(plt::Plot, x::Number, y::Number)
    return _plots_extend_xy!(plt, 1, x, y)
end

function Base.push!(plt::Plot, x::Number, y::Number, z::Number)
    return _plots_extend_xyz!(plt, 1, x, y, z)
end

function _plots_copy_levels(levels)
    if levels === nothing || isa(levels, Number)
        return levels
    end
    return _plots_copy_data(levels)
end

function _plots_copy_series(s)
    z = s.z === nothing ? nothing : _plots_copy_data(s.z)
    return Series(_plots_copy_data(s.x), _plots_copy_data(s.y), z, s.label, s.seriestype,
                  _plots_copy_levels(s.levels))
end

function _plots_copy_series_list(series)
    snap = Any[]
    for s in series
        push!(snap, _plots_copy_series(s))
    end
    return snap
end

# `frame(anim, plt)` deep-copies the plot's series into a standalone snapshot and
# appends it to the animation. Copying is required: the series vectors are mutated
# in place by `push!`, so sharing them would make every frame show the final state.
# Axis limits and reference lines (Issue #7850) are carried into the snapshot.
function frame(anim::Animation, plt::Plot)
    snap = _plots_copy_series_list(plt.series)
    push!(anim.frames, Plot(snap, plt.backend, plt.aspect_ratio, plt.title,
                            plt.xlims, plt.ylims, plt.hlines, plt.vlines))
    return anim
end

function frame(anim::Animation)
    return frame(anim, current())
end

# `gif(anim)` wraps the collected frames into an `AnimatedGif`; the Rust artifact
# pipeline detects this type and emits a Plotly frames animation.
function gif(anim::Animation; fps=20)
    return AnimatedGif(anim.frames, fps)
end

# `frame(anim, should::Bool)` captures a frame only when `should` is true. The
# `@animate`/`@gif` macros (Issue #7272) emit a call to this overload once per loop
# iteration rather than building an `if` node themselves: the sjulia macro runtime
# can splice a built/parsed boolean expression as a *call argument*, but not as the
# condition of a macro-constructed `Expr(:if, …)`. A bare `@animate` (no modifier)
# passes `true`, capturing every iteration like before (Issue #6355). This reuses the
# already-exported `frame` (the macro introduces an unqualified call that must resolve
# in the caller's scope) rather than a separate non-exported helper. The `::Bool`
# signature keeps it distinct from `frame(anim, plt::Plot)`.
function frame(anim::Animation, should::Bool)
    if should
        frame(anim)
    end
    return nothing
end

# How the `@animate`/`@gif` macros rebuild the loop (Issues #6355 / #7272)
# ------------------------------------------------------------------------
# Each macro appends a per-iteration capture call and counter bump to the user's
# `for`/`while` body, then splices the loop in `esc`-ed so the user's variables
# resolve in the caller's scope. `forloop.args[1]` is the loop binding (`i in 1:n` /
# the while condition), `forloop.args[2]` is the body block. `_anim`/`_anim_counter`
# are quote-locals; they still resolve inside the `esc`-ed loop (the mechanism the
# original single-frame `@animate` relied on, Issue #6355).
#
# The capture predicate `<should>` mirrors upstream Plots.jl's `_animate`
# (PlotsBase/src/animation.jl):
#
#   (no modifier)  ->  true                          capture every iter (Issue #6355)
#   every N        ->  mod1(_anim_counter, N) == 1   capture iters 1, N+1, 2N+1, …
#   when c         ->  c                              capture when the condition holds
#
# `<should>` is spliced as a *call argument* to `frame`, never as the condition of a
# macro-built `Expr(:if, …)`: the sjulia macro runtime can round-trip a built/parsed
# expression as a call argument but not as an `if` condition (so the gating lives in
# the `frame(anim, ::Bool)` runtime overload). The `==` callee is `Symbol("==")`
# because `:(==)` round-trips as an operator expression, not a callable symbol. `args`
# is the macro's trailing varargs (`()`, `(:every, N)`, or `(:when, c)`); a single
# registry slot per macro name means `@animate`/`@gif` must accept both arities
# through one varargs method. The logic is inlined into each macro body because macro
# expansion runs in a compile-time VM that only sees builtins (`Expr`, `Symbol`, …),
# not the package's own helpers — but the spliced `frame` call resolves at runtime in
# the caller's scope because `frame` is exported.

# `@animate for … end` collects one frame per iteration into an `Animation`; a
# trailing `every N` / `when cond` modifier samples a subset (Issue #7272). The
# block's value is the `Animation`.
macro animate(forloop, args...)
    # Workaround: hold the frame counter in a `Ref` so the loop mutates storage. (Issue #9476)
    # contents (`_anim_counter[] = …`, a `setindex!` — not a rebinding of the
    # `_anim_counter` name) rather than reassigning a plain top-level counter.
    # Under strict file-mode soft scope (Issue #9210), a plain counter assigned
    # before the loop and `+=`-ed inside it is soft-scope-localized to a fresh
    # loop-local, so its read-before-write raises `UndefVarError` — this broke
    # the animation samples once the C ABI / WASM hosts went strict (Issue #9283).
    # Upstream Plots sidesteps it with macro hygiene (a `gensym` local counter);
    # the natural sjulia analogue would emit `global _anim_counter` or wrap the
    # counter in a `let`, but sjulia's macro runtime rejects both in expansion
    # output (Issue #9476). The `Ref` mutation is the upstream-valid shape the
    # runtime accepts. See docs/vm/WORKAROUNDS.md (Issue #9476 / #9283).
    should = length(args) >= 2 ?
        (args[1] === :every ?
            Expr(:call, Symbol("=="), Expr(:call, :mod1, :(_anim_counter[]), args[2]), 1) :
            args[2]) :
        true
    newbody = Expr(:block, forloop.args[2], Expr(:call, :frame, :_anim, should), :(_anim_counter[] = _anim_counter[] + 1))
    newloop = Expr(forloop.head, forloop.args[1], newbody)
    quote
        local _anim = Animation()
        local _anim_counter = Ref(1)
        $(esc(newloop))
        _anim
    end
end

# --- Axis limits and reference lines (Issue #7850) ---

# `title!(s)` / `title!(plt, s)` — update the title of the current or given plot.
# Returns the updated Plot and makes it current.  The non-bang getter `title(plt)`
# is not exported (upstream Plots.jl does not export it either).
function title!(s::AbstractString)
    plt = current()
    xl = _plots_current_xlims()
    yl = _plots_current_ylims()
    hl = _plots_current_hlines()
    vl = _plots_current_vlines()
    _plots_set_current!(plt.series, plt.aspect_ratio, s, xl, yl, hl, vl)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, s, xl, yl, hl, vl)
end

function title!(plt::Plot, s::AbstractString)
    _plots_set_current!(plt.series, plt.aspect_ratio, s,
                        plt.xlims, plt.ylims, plt.hlines, plt.vlines)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, s,
                plt.xlims, plt.ylims, plt.hlines, plt.vlines)
end

# `xlims!(a, b)` / `xlims!(plt, a, b)` — set explicit x-axis display range.
# Tuple form `xlims!((a, b))` is also accepted.  Returns the updated Plot.
function xlims!(a::Number, b::Number)
    plt = current()
    xl = (Float64(a), Float64(b))
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        xl, _plots_current_ylims(),
                        _plots_current_hlines(), _plots_current_vlines())
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                xl, plt.ylims, plt.hlines, plt.vlines)
end

function xlims!(t::Tuple)
    return xlims!(t[1], t[2])
end

function xlims!(plt::Plot, a::Number, b::Number)
    xl = (Float64(a), Float64(b))
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        xl, plt.ylims, plt.hlines, plt.vlines)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                xl, plt.ylims, plt.hlines, plt.vlines)
end

function xlims!(plt::Plot, t::Tuple)
    return xlims!(plt, t[1], t[2])
end

# `xlims(plt)` — getter: returns the explicit range if set, otherwise computes
# `(min, max)` from the series data (matching upstream Plots.jl getter semantics).
# `xlims()` (no args) queries the current plot.
function xlims(plt::Plot)
    if plt.xlims !== nothing
        return plt.xlims
    end
    all_x = Float64[]
    for s in plt.series
        for x in s.x
            v = Float64(x)
            if isfinite(v)
                push!(all_x, v)
            end
        end
    end
    if isempty(all_x)
        return (0.0, 1.0)
    end
    return (minimum(all_x), maximum(all_x))
end

function xlims()
    return xlims(current())
end

# `ylims!` / `ylims` — same pattern as xlims but for the y axis.
function ylims!(a::Number, b::Number)
    plt = current()
    yl = (Float64(a), Float64(b))
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        _plots_current_xlims(), yl,
                        _plots_current_hlines(), _plots_current_vlines())
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                plt.xlims, yl, plt.hlines, plt.vlines)
end

function ylims!(t::Tuple)
    return ylims!(t[1], t[2])
end

function ylims!(plt::Plot, a::Number, b::Number)
    yl = (Float64(a), Float64(b))
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        plt.xlims, yl, plt.hlines, plt.vlines)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                plt.xlims, yl, plt.hlines, plt.vlines)
end

function ylims!(plt::Plot, t::Tuple)
    return ylims!(plt, t[1], t[2])
end

function ylims(plt::Plot)
    if plt.ylims !== nothing
        return plt.ylims
    end
    all_y = Float64[]
    for s in plt.series
        for y in s.y
            v = Float64(y)
            if isfinite(v)
                push!(all_y, v)
            end
        end
    end
    if isempty(all_y)
        return (0.0, 1.0)
    end
    return (minimum(all_y), maximum(all_y))
end

function ylims()
    return ylims(current())
end

# `hline!(ys)` / `hline!(plt, ys)` — append horizontal reference lines (y values)
# to the current or given plot.  Single-value and vector forms both accepted.
# `hline(ys)` (non-bang) creates a new plot carrying only the reference lines
# (no data series), matching upstream Plots.jl where `hline` builds a standalone
# horizontal-line plot.
function hline!(ys::AbstractVector)
    plt = current()
    new_hl = copy(_plots_current_hlines())
    for y in ys
        push!(new_hl, Float64(y))
    end
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        _plots_current_xlims(), _plots_current_ylims(),
                        new_hl, _plots_current_vlines())
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                plt.xlims, plt.ylims, new_hl, plt.vlines)
end

function hline!(y::Number)
    return hline!([Float64(y)])
end

function hline!(plt::Plot, ys::AbstractVector)
    new_hl = copy(plt.hlines)
    for y in ys
        push!(new_hl, Float64(y))
    end
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        plt.xlims, plt.ylims, new_hl, plt.vlines)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                plt.xlims, plt.ylims, new_hl, plt.vlines)
end

function hline!(plt::Plot, y::Number)
    return hline!(plt, [Float64(y)])
end

function hline(ys::AbstractVector)
    hl = Float64[Float64(y) for y in ys]
    series = Any[]
    _plots_set_current!(series, :auto, "", nothing, nothing, hl, Float64[])
    return Plot(series, :text, :auto, "", nothing, nothing, hl, Float64[])
end

function hline(y::Number)
    return hline([Float64(y)])
end

# `vline!` / `vline` — same pattern as hline but for vertical reference lines
# (x values).
function vline!(xs::AbstractVector)
    plt = current()
    new_vl = copy(_plots_current_vlines())
    for x in xs
        push!(new_vl, Float64(x))
    end
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        _plots_current_xlims(), _plots_current_ylims(),
                        _plots_current_hlines(), new_vl)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                plt.xlims, plt.ylims, plt.hlines, new_vl)
end

function vline!(x::Number)
    return vline!([Float64(x)])
end

function vline!(plt::Plot, xs::AbstractVector)
    new_vl = copy(plt.vlines)
    for x in xs
        push!(new_vl, Float64(x))
    end
    _plots_set_current!(plt.series, plt.aspect_ratio, plt.title,
                        plt.xlims, plt.ylims, plt.hlines, new_vl)
    return Plot(plt.series, plt.backend, plt.aspect_ratio, plt.title,
                plt.xlims, plt.ylims, plt.hlines, new_vl)
end

function vline!(plt::Plot, x::Number)
    return vline!(plt, [Float64(x)])
end

function vline(xs::AbstractVector)
    vl = Float64[Float64(x) for x in xs]
    series = Any[]
    _plots_set_current!(series, :auto, "", nothing, nothing, Float64[], vl)
    return Plot(series, :text, :auto, "", nothing, nothing, Float64[], vl)
end

function vline(x::Number)
    return vline([Float64(x)])
end

# `@gif for … end` is `@animate` followed by an immediate `gif(…)`; it accepts the
# same trailing `every N` / `when cond` modifier (Issue #7272).
macro gif(forloop, args...)
    # Workaround: `Ref` frame counter — see the `@animate` macro above. (Issue #9476)
    # full rationale (strict file-mode soft scope localizes a plain top-level
    # counter; sjulia's macro runtime rejects `global`/`let` in expansion output).
    # Issue #9476 / #9283, docs/vm/WORKAROUNDS.md.
    should = length(args) >= 2 ?
        (args[1] === :every ?
            Expr(:call, Symbol("=="), Expr(:call, :mod1, :(_anim_counter[]), args[2]), 1) :
            args[2]) :
        true
    newbody = Expr(:block, forloop.args[2], Expr(:call, :frame, :_anim, should), :(_anim_counter[] = _anim_counter[] + 1))
    newloop = Expr(forloop.head, forloop.args[1], newbody)
    quote
        local _anim = Animation()
        local _anim_counter = Ref(1)
        $(esc(newloop))
        gif(_anim)
    end
end
