# =============================================================================
# deepcopy
# =============================================================================
# Based on Julia's base/deepcopy.jl.
#
# The recursive copy engine is still a VM boundary because it needs access to
# the heap-backed value model. Public deepcopy is a Julia wrapper so it behaves
# as an ordinary callable Base function.

deepcopy(x) = _deepcopy(x)
