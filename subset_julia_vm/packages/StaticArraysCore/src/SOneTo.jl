struct SOneTo
    n::Int64
end

Base.length(r::SOneTo) = r.n
Base.first(r::SOneTo) = 1
Base.last(r::SOneTo) = r.n
