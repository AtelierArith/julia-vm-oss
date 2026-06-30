macro make_incrementer()
    :(x -> x + 1)
end

f = @make_incrementer()
f(41) == 42 || error("macro-returned arrow function is not callable")

true
