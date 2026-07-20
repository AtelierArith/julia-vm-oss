struct Series
    x
    y
    z
    label
    seriestype
    levels
    Series(x, y, label) = new(x, y, nothing, label, :line, nothing)
    Series(x, y, label, seriestype) = new(x, y, nothing, label, seriestype, nothing)
    Series(x, y, z, label, seriestype) = new(x, y, z, label, seriestype, nothing)
    Series(x, y, z, label, seriestype, levels) = new(x, y, z, label, seriestype, levels)
end

# `title` is the 4th field (Issue #7030). Fields 5-8 added by Issue #7850:
# `xlims`/`ylims` are `nothing` (auto) or a `(lo, hi)` tuple; the Rust pipeline
# reads them from `values[4]`/`values[5]` to inject `xaxis.range`/`yaxis.range`.
# `hlines`/`vlines` hold Float64 reference-line values; Rust reads `values[6]`/
# `values[7]` and emits Plotly `shapes`.  Existing positional reads (`values[0]`
# series, `values[2]` aspect_ratio, `values[3]` title) are unaffected.
struct Plot
    series
    backend
    aspect_ratio
    title
    xlims
    ylims
    hlines
    vlines
    Plot(series, backend) = new(series, backend, :auto, "", nothing, nothing, Float64[], Float64[])
    Plot(series, backend, aspect_ratio) = new(series, backend, aspect_ratio, "", nothing, nothing, Float64[], Float64[])
    Plot(series, backend, aspect_ratio, title) = new(series, backend, aspect_ratio, title, nothing, nothing, Float64[], Float64[])
    Plot(series, backend, aspect_ratio, title, xlims) = new(series, backend, aspect_ratio, title, xlims, nothing, Float64[], Float64[])
    Plot(series, backend, aspect_ratio, title, xlims, ylims) = new(series, backend, aspect_ratio, title, xlims, ylims, Float64[], Float64[])
    Plot(series, backend, aspect_ratio, title, xlims, ylims, hlines) = new(series, backend, aspect_ratio, title, xlims, ylims, hlines, Float64[])
    Plot(series, backend, aspect_ratio, title, xlims, ylims, hlines, vlines) = new(series, backend, aspect_ratio, title, xlims, ylims, hlines, vlines)
end

const _CURRENT_SERIES = Any[]
const _CURRENT_ASPECT_RATIO = Any[]
const _CURRENT_TITLE = Any[]
const _CURRENT_XLIMS = Any[]
const _CURRENT_YLIMS = Any[]
const _CURRENT_HLINES = Any[]
const _CURRENT_VLINES = Any[]

# --- Animation support (Issue #6355) ---
#
# Upstream Plots.jl writes each frame to a PNG on disk and stitches them with
# FFmpeg. iOS / WASM have neither a filesystem nor FFmpeg, so instead of file
# paths `Animation.frames` holds in-memory `Plot` snapshots, and `gif` returns an
# `AnimatedGif` that the Rust artifact pipeline turns into a native Plotly frames
# animation. No file I/O is performed.
struct Animation
    frames
    Animation() = new(Any[])
end

struct AnimatedGif
    frames
    fps
    AnimatedGif(frames) = new(frames, 20)
    AnimatedGif(frames, fps) = new(frames, fps)
end
