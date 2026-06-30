struct KeywordForwardingRecorder8375
    value
end

function keyword_forwarding_target_8375(args...; segbuf=nothing, kws...)
    return (segbuf, args, length(kws))
end

function keyword_forwarding_with_kwargs_splat_8375(args...; segbuf=nothing, kws...)
    return keyword_forwarding_target_8375(
        args...;
        segbuf=KeywordForwardingRecorder8375(segbuf),
        kws...,
    )
end

function keyword_forwarding_without_kwargs_splat_8375(args...; segbuf=nothing, kws...)
    return keyword_forwarding_target_8375(
        args...;
        segbuf=KeywordForwardingRecorder8375(segbuf),
    )
end

with_splat = keyword_forwarding_with_kwargs_splat_8375(1, 2)
without_splat = keyword_forwarding_without_kwargs_splat_8375(3, 4)

with_splat[1] isa KeywordForwardingRecorder8375 &&
    length(with_splat[2]) == 2 &&
    with_splat[3] == 0 &&
    without_splat[1] isa KeywordForwardingRecorder8375 &&
    length(without_splat[2]) == 2 &&
    without_splat[3] == 0
