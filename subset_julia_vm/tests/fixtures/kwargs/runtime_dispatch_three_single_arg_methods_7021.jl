kwdispatch_7021(x::Function; aspect_ratio=:auto) = aspect_ratio
kwdispatch_7021(x::Vector; aspect_ratio=:auto) = aspect_ratio
kwdispatch_7021(x::Number; aspect_ratio=:auto) = aspect_ratio

kwdispatch_7021(sin, aspect_ratio=:equal) == :equal &&
    kwdispatch_7021([1, 2, 3], aspect_ratio=:equal) == :equal &&
    kwdispatch_7021(1, aspect_ratio=:equal) == :equal
