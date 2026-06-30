struct ValueParamWrapper{T,N,P,MI}
    parent::P
    dims::Tuple
end

function make_value_param_wrapper(a::P, dims::Tuple) where P
    T = eltype(a)
    return ValueParamWrapper{T,2,P,Tuple{}}(a, dims)
end

arr = [1, 2, 3, 4]
w = make_value_param_wrapper(arr, (2, 2))

string(typeof(w)) == "ValueParamWrapper{Int64, 2, Vector{Int64}, Tuple{}}" &&
    w.parent == arr &&
    w.dims == (2, 2)
