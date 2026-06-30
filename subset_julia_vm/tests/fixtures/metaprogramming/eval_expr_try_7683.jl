println(eval(:(try error() catch; 123 finally end)))
println(eval(:(try 7 catch; 1 finally 9 end)))

true
