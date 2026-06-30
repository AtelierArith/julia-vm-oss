module Interact

# A minimal, MVP subset of Interact.jl for SubsetJuliaVM (Issue #7275).
#
# Upstream Interact.jl builds *reactive* widgets (sliders, dropdowns, checkboxes)
# backed by a two-way Observables/WebIO transport: moving a slider re-runs the body
# and live-updates the displayed output. SubsetJuliaVM is a no-JIT, one-shot REPL on
# iOS / Web with no reactive runtime and no widget-to-VM callback channel, so a
# faithful reactive `@manipulate` is out of scope.
#
# Instead this MVP gives the *discrete, single-variable* `@manipulate for var =
# choices … end` form a static-but-interactive rendering: the body is evaluated once
# per discrete choice, and the per-choice `Plots.Plot`s are combined into ONE Plotly
# figure with a dropdown (`updatemenus`) that toggles visibility between the
# per-choice trace groups. This flows through the existing display-artifact pipeline
# (the same `application/vnd.plotly+json` path `plot`/`scatter` already use), so it
# renders on iOS / Web without a reactive runtime.
#
#     using Interact, Plots
#     datasets = Dict(:some => [1.0, 4.0, 9.0], :other => [2.0, 3.0, 5.0])
#     @manipulate for dataset = [:some, :other]
#         scatter(datasets[dataset])
#     end
#
# Supported (MVP):
#   * a single control: `for var = <range or vector of choices>` — a range renders as
#     a static Plotly slider, anything else as a dropdown (Issue #7338)
#   * multiple controls: `for a = …, b = …` — the cartesian product of choices as one
#     combined dropdown, labelled `a=<va>, b=<vb>, …` (Issue #7344)
#   * a plot-producing body (each combination's body must return a `Plots.Plot`;
#     non-plot bodies error clearly, Issue #7338)
#
# Deferred (Phase 2 — see UNIMPLEMENTED.md / Issue #7275):
#   * true reactivity / live re-evaluation, two-way FFI, native controls
#   * N *independent* controls (we collapse the product into one combined control)
#   * non-plot bodies (numbers / strings / arbitrary HTML) rendered as values

using Plots

include("types.jl")

# How the `@manipulate` macro rebuilds the loop (Issue #7275)
# ----------------------------------------------------------
# Mirrors the construction proven by `Plots.@animate`/`@gif` (api.jl): build the
# loop with `Expr` (the macro-expansion VM only sees builtins like `Expr`/`Symbol`,
# not package helpers), then splice it `esc`-ed so the user's `var`/`choices`/body
# resolve in the caller's scope. `_interact_*` are quote-locals; they still resolve
# inside the `esc`-ed loop (the same mechanism `@animate`'s `_anim` relies on).
#
# `forloop.args[1]` is the loop binding `Expr(:=, var, choices)`; `forloop.args[2]`
# is the body block. We reuse the binding verbatim (so `var = choices` keeps the
# user's exact iteration semantics) and replace the body with a block that:
#   1. evaluates the user's body and `push!`es its value (a `Plot`) into `_interact_plots`
#   2. `push!`es `string(var)` into `_interact_labels` as the dropdown label
# The block's value is a `Manipulate`, which the Rust artifact pipeline renders as a
# Plotly-dropdown figure (see types.jl).

# Map a `@manipulate` choices value to its static control kind (Issue #7338): an
# `AbstractRange` renders as a continuous slider (mirrors upstream `widget()`
# `AbstractRange → slider`); everything else keeps the discrete dropdown (≈ upstream
# `togglebuttons`). This lives in a real function (not inline in the macro expansion)
# because sjulia macro expansion evaluates a ternary in argument position to
# `nothing`; it is exported so the expansion can call it as a bare name (qualified
# `Interact._manipulate_control(...)` call targets are also rejected by expansion).
manipulate_control(choices) = isa(choices, AbstractRange) ? :slider : :dropdown

macro manipulate(forloop)
    spec = forloop.args[1]     # Expr(:=, var, choices) OR Expr(:block, bindings…)
    userbody = forloop.args[2] # the per-choice body block

    # Multiple simultaneous controls (`for a = …, b = …`, Issue #7344). Upstream
    # parses these as `Expr(:for, Expr(:block, :(a=…), :(b=…)), body)` (quote of
    # multiple for-bindings landed in #7343). Upstream gives each variable its own
    # reactive control and re-evaluates on any change; with no reactive runtime we
    # approximate that as ONE static dropdown over the *cartesian product* of all
    # choices — every combination is selectable, labelled `a=<va>, b=<vb>, …`. The
    # body is evaluated once per combination via nested loops.
    if spec.head == :block
        bindings = spec.args   # each is Expr(:=, var, choices)
        # Combined label built from fixed-arity `string(...)` calls (the macro VM does
        # not support splatting, and avoids `?:`/`=== nothing` which the expansion-time
        # VM mis-evaluates): seed with the first binding `string("a=", a)`, then fold in
        # `string(prev, ", ", "b=", b)` for the rest.
        v1 = bindings[1].args[1]
        labelexpr = Expr(:call, :string, string(v1) * "=", v1)
        for i in 2:length(bindings)
            v = bindings[i].args[1]
            labelexpr = Expr(:call, :string, labelexpr, ", " * string(v) * "=", v)
        end
        capture = Expr(:call, :push!, :_interact_plots, userbody)
        labelpush = Expr(:call, :push!, :_interact_labels, labelexpr)
        # Nest the loops inside-out so the innermost body sees every loop variable.
        loopexpr = Expr(:block, capture, labelpush)
        for i in length(bindings):-1:1
            loopexpr = Expr(:for, bindings[i], loopexpr)
        end
        return quote
            local _interact_plots = Any[]
            local _interact_labels = Any[]
            $(esc(loopexpr))
            for _interact_p in _interact_plots
                isa(_interact_p, Plot) || error(
                    "@manipulate body must return a Plots.Plot (got $(typeof(_interact_p))); " *
                    "non-plot bodies are not yet supported, see Issue #7338",
                )
            end
            # Cartesian-product controls render as a single combined dropdown.
            Manipulate(_interact_plots, _interact_labels, :dropdown)
        end
    end

    # Single control: `for var = choices`.
    binding = spec                 # Expr(:=, var, choices)
    var = binding.args[1]          # the loop variable (Symbol)
    choices = binding.args[2]      # the choices expression (range / array / …)
    # Per-iteration body: push the body's value (a Plot) and its label.
    capture = Expr(:call, :push!, :_interact_plots, userbody)
    labelpush = Expr(:call, :push!, :_interact_labels, Expr(:call, :string, var))
    newbody = Expr(:block, capture, labelpush)
    newloop = Expr(forloop.head, binding, newbody)
    # Validate that every captured value is a `Plot` before building the
    # `Manipulate` (Issue #7338). Upstream Interact renders non-plot bodies
    # (numbers/strings/HTML) through a reactive widget runtime, which is out of
    # scope for this static-Plotly MVP; without this check a non-plot body silently
    # built a `Manipulate` whose `plots` held raw values (e.g. `Any[1, 4, 9]`),
    # evaluated to exit 0, and rendered nothing. The check belongs in `Manipulate`'s
    # inner constructor, but sjulia ignores inner constructor bodies (Issue #7345),
    # so it lives here in the expansion (runs in the `Interact` module scope, where
    # `Plot` resolves via `using Plots`).
    #
    # Control kind via `manipulate_control($(esc(choices)))` (Issue #7338): the
    # exported helper resolves as a bare name in the caller's scope (where the
    # expansion runs) and its argument is evaluated there too. This re-evaluates
    # `choices` once more than the loop, but only its *type* is used (stable) and
    # `choices` is a cheap range/array literal in practice; reading a captured
    # quote-local back here is unreliable under sjulia macro hygiene, so we avoid it.
    quote
        local _interact_plots = Any[]
        local _interact_labels = Any[]
        $(esc(newloop))
        for _interact_p in _interact_plots
            isa(_interact_p, Plot) || error(
                "@manipulate body must return a Plots.Plot (got $(typeof(_interact_p))); " *
                "non-plot bodies are not yet supported, see Issue #7338",
            )
        end
        Manipulate(
            _interact_plots,
            _interact_labels,
            manipulate_control($(esc(choices))),
        )
    end
end

export Manipulate, @manipulate, manipulate_control

end # module Interact
