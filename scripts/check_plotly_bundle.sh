#!/usr/bin/env bash
# check_plotly_bundle.sh
#
# Verify every bundled Plotly.js registers all trace types the VM emits.
#
# The VM's Plotly JSON generator (subset_julia_vm/src/plotting/plotly.rs) emits
# these trace `type`s: scatter (2D line/scatter), bar, heatmap, scatter3d and
# surface. Plotly ships *partial* bundles: the `gl3d` build has the 3D traces
# (scatter3d/surface) but NOT the cartesian `bar`/`heatmap` modules. When Plotly
# receives an unregistered trace type it silently coerces it to `scatter`, so a
# `bar` plot renders as a line graph (Issue #6850). The only stock bundle that
# carries BOTH the 3D and the cartesian traces is the full `plotly.min.js`.
#
# This guards against regressing any of the three shipped bundles (iOS, Web,
# Flutter) back to a partial build that drops a trace module.
#
# Usage: run from the repository root
#   bash scripts/check_plotly_bundle.sh
#
# Exit code: 0 = every bundle registers every required trace, 1 = a trace is
# missing from some bundle.

set -euo pipefail

# Bundles shipped to each host. All must render the same set of plot types.
BUNDLES="
SubsetJuliaVMApp/SubsetJuliaVMApp/Resources/plotly.min.js
web/plotly.min.js
mobile/assets/plotly/plotly.min.js
"

# Trace types emitted by subset_julia_vm/src/plotting/plotly.rs (render_trace).
REQUIRED_TRACES="scatter bar heatmap scatter3d surface"

failed=0

for bundle in $BUNDLES; do
    if [[ ! -f "$bundle" ]]; then
        echo "ERROR: bundled Plotly.js not found: $bundle"
        echo "Run this script from the repository root."
        failed=1
        continue
    fi
    for trace in $REQUIRED_TRACES; do
        # Plotly registers each trace module as moduleType:"trace",name:"<type>".
        if ! grep -q "moduleType:\"trace\",name:\"$trace\"" "$bundle"; then
            echo "ERROR: $bundle does not register the \"$trace\" trace module."
            failed=1
        fi
    done
done

if [[ $failed -ne 0 ]]; then
    echo ""
    echo "A bundled Plotly.js is missing a trace module the VM emits, so that plot"
    echo "type silently falls back to a scatter (line) trace on the host (Issue #6850)."
    echo "Fix: replace the partial bundle with the full plotly.min.js, e.g."
    echo "  curl -sL https://cdn.plot.ly/plotly-2.35.2.min.js -o <bundle>"
    echo "(keep all three host bundles on the same full build)."
    exit 1
fi

echo "OK: every bundled Plotly.js registers all VM trace types ($REQUIRED_TRACES) (Issue #6850)."
