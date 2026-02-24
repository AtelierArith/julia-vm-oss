# This file is a part of SubsetJuliaVM.
# Based on Julia's base/docs/utils.jl

# =============================================================================
# Text / HTML objects
# =============================================================================
# These types provide wrapper objects for rich text content.
# They are commonly used with string literals: html"..." and text"..."

# =============================================================================
# HTML Type
# =============================================================================

"""
`HTML(s)`: Create an object that renders `s` as html.

    HTML("<div>foo</div>")

# Examples
```julia
julia> html"<b>bold</b>"
HTML{String}("<b>bold</b>")
```
"""
mutable struct HTML{T}
    content::T
end

# Constructor that wraps content in HTML
function HTML(s::String)
    HTML{String}(s)
end

"""
    html_str(s) -> HTML{String}

Create an `HTML` object from a literal string.
This function is called when using the `html"..."` string literal syntax.

# Examples
```julia
julia> html"Julia"
HTML{String}("Julia")
```
"""
function html_str(s::String)
    HTML{String}(s)
end

# =============================================================================
# Text Type
# =============================================================================

"""
`Text(s)`: Create an object that renders `s` as plain text.

    Text("foo")

# Examples
```julia
julia> text"hello world"
hello world
```
"""
mutable struct Text{T}
    content::T
end

# Constructor that wraps content in Text
function Text(s::String)
    Text{String}(s)
end

"""
    text_str(s) -> Text{String}

Create a `Text` object from a literal string.
This function is called when using the `text"..."` string literal syntax.

# Examples
```julia
julia> text"Julia"
Julia
```
"""
function text_str(s::String)
    Text{String}(s)
end

# =============================================================================
# Equality and Hashing
# =============================================================================

function Base.:(==)(h1::HTML{T}, h2::HTML{T}) where T
    h1.content == h2.content
end

function Base.:(==)(t1::Text{T}, t2::Text{T}) where T
    t1.content == t2.content
end
