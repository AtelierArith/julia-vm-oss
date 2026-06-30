Base.isexpr(:((a, b)), :tuple) && !Base.isexpr(:(a + b), :tuple)
