module FunctionValue7955

S(x) = x + 1

end

module TypeOwner7955

struct S
    value
end

abstract type Module end

struct Box <: Module
end

end

f = FunctionValue7955.S

ok = f(41) == 42 &&
     FunctionValue7955.S(1) == 2 &&
     TypeOwner7955.Box() isa TypeOwner7955.Module &&
     TypeOwner7955.Module !== Module
println(ok)
ok
