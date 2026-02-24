# Test short function syntax with where clause
triple(x::T) where T<:Number = x * 3
triple(2.0)
