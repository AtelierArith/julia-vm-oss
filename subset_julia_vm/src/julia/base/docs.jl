# =============================================================================
# docs.jl - Minimal documentation surface
# =============================================================================
# Upstream Julia's Base.Docs stores rich Markdown metadata. SubsetJuliaVM only
# exposes a minimal Docs module while lowering stores plain-text docstrings in
# hidden globals for @doc retrieval.

module Docs

export doc, doc!

function doc!(name::String, text)
    return string(text)
end

function doc(name::String)
    return nothing
end

end # module Docs
