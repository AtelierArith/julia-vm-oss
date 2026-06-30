function idx(arr, s)
    a = arr[1]
    sub = arr[1:2]
    c = s[1]
    ss = s[1:2]
    return (a, length(sub), c, ss)
end

r = idx([10, 20, 30], "hello")
println(r[1])
println(r[2])
println(r[3])
println(r[4])
println(typeof([10, 20, 30][1]))
println(typeof("hello"[1]))
println(typeof("hello"[1:2]))

true
