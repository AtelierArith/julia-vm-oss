# Test simple varargs - just print what we receive

function show_args(args...)
    println("args type: ", typeof(args))
    println("args length: ", length(args))
    length(args)
end

result = show_args(1, 2, 3)
result == 3
