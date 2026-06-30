# `Manipulate` is the value produced by `@manipulate for var = choices … end`
# (Issue #7275). It is the dropdown counterpart of `Plots.AnimatedGif`: rather than
# building a reactive widget runtime (out of scope for the MVP — see Interact.jl),
# `@manipulate` evaluates the body once per discrete choice and stores the resulting
# per-choice `Plot` snapshots plus their labels. The Rust artifact pipeline detects
# this struct and emits a single static Plotly figure whose `updatemenus` dropdown
# switches `visible` between the per-choice trace groups (mirroring how `AnimatedGif`
# becomes a Plotly *frames* animation). No file I/O and no FFI callbacks are used, so
# it renders on iOS / Web through the existing display-artifact path.
#
# Fields:
#   plots   — `Vector` of the per-choice `Plots.Plot` snapshots (first = default visible)
#   labels  — `Vector` of `String` labels, one per choice (`string(choice)`)
#   control — `Symbol` picking the static control: `:slider` (continuous, for an
#             `AbstractRange` choice — mirrors upstream `widget()` `AbstractRange →
#             slider`, Issue #7338) or `:dropdown` (discrete, the default for arrays/
#             other iterables, ≈ upstream's `togglebuttons`).
#
# NOTE: the "every captured value must be a `Plots.Plot`" check (Issue #7338) would
# naturally live in an inner constructor here, but sjulia silently ignores inner
# constructor bodies (Issue #7345), so the validation is performed in the
# `@manipulate` macro expansion instead (see Interact.jl). Revisit once #7345 lands.
struct Manipulate
    plots
    labels
    control
end

# Backward-compatible 2-arg form (defaults to the dropdown control). An *outer*
# constructor, not an inner one — sjulia ignores inner constructor bodies (#7345).
Manipulate(plots, labels) = Manipulate(plots, labels, :dropdown)
