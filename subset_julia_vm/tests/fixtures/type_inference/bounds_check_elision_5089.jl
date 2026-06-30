using Test

function collect_eachindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in eachindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in 1:length(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_base_eachindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.eachindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_base_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in 1:Base.length(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_lastindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in 1:lastindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_first_lastindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in firstindex(arr):lastindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_base_first_lastindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.firstindex(arr):Base.lastindex(arr)
        push!(out, arr[i])
    end
    return out
end

function collect_axes_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in axes(arr, 1)
        push!(out, arr[i])
    end
    return out
end

function collect_base_axes_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.axes(arr, 1)
        push!(out, arr[i])
    end
    return out
end

function collect_base_oneto_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.OneTo(length(arr))
        push!(out, arr[i])
    end
    return out
end

function collect_base_oneto_function_length_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in Base.oneto(length(arr))
        push!(out, arr[i])
    end
    return out
end

function collect_direct_getindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in eachindex(arr)
        push!(out, getindex(arr, i))
    end
    return out
end

function collect_base_getindex_inbounds_5089(arr::Vector{Int32})
    out = Int32[]
    for i in eachindex(arr)
        push!(out, Base.getindex(arr, i))
    end
    return out
end

function increment_eachindex_store_inbounds_5089(arr::Vector{Float64})
    for i in eachindex(arr)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_length_store_inbounds_5089(arr::Vector{Float64})
    for i in 1:length(arr)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_axes_store_inbounds_5089(arr::Vector{Float64})
    for i in axes(arr, 1)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_base_axes_store_inbounds_5089(arr::Vector{Float64})
    for i in Base.axes(arr, 1)
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_base_oneto_lastindex_store_inbounds_5089(arr::Vector{Float64})
    for i in Base.OneTo(Base.lastindex(arr))
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_base_oneto_function_lastindex_store_inbounds_5089(arr::Vector{Float64})
    for i in Base.oneto(Base.lastindex(arr))
        arr[i] = arr[i] + 1.5
    end
    return arr
end

function increment_eachindex_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in eachindex(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

function increment_length_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in 1:length(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

function increment_base_lastindex_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in 1:Base.lastindex(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

function increment_first_lastindex_setindex_call_inbounds_5089(arr::Vector{Float64})
    for i in firstindex(arr):lastindex(arr)
        setindex!(arr, arr[i] + 2.0, i)
    end
    return arr
end

@testset "bounds-check elision proof patterns (Issue #5089)" begin
    values = Int32[10, 20, 30]
    @test collect_eachindex_inbounds_5089(values) == values
    @test collect_length_inbounds_5089(values) == values
    @test collect_base_eachindex_inbounds_5089(values) == values
    @test collect_base_length_inbounds_5089(values) == values
    @test collect_lastindex_inbounds_5089(values) == values
    @test collect_first_lastindex_inbounds_5089(values) == values
    @test collect_base_first_lastindex_inbounds_5089(values) == values
    @test collect_axes_inbounds_5089(values) == values
    @test collect_base_axes_inbounds_5089(values) == values
    @test collect_base_oneto_length_inbounds_5089(values) == values
    @test collect_base_oneto_function_length_inbounds_5089(values) == values
    @test collect_direct_getindex_inbounds_5089(values) == values
    @test collect_base_getindex_inbounds_5089(values) == values

    @test increment_eachindex_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_length_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_axes_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_base_axes_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_base_oneto_lastindex_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_base_oneto_function_lastindex_store_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[2.5, 3.5, 4.5]
    @test increment_eachindex_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
    @test increment_length_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
    @test increment_base_lastindex_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
    @test increment_first_lastindex_setindex_call_inbounds_5089(Float64[1.0, 2.0, 3.0]) == Float64[3.0, 4.0, 5.0]
end

true
