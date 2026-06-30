Base.:(==)(a::String, b::String) = false
Base.:(==)(a::Symbol, b::Symbol) = false
Base.:(==)(a::Bool, b::Bool) = false
Base.:(==)(a::Char, b::Char) = false
Base.:(==)(a::Type{Int64}, b::Type{Int64}) = false

same_4298(a::Any, b::Any) = a == b
notsame_4298(a::Any, b::Any) = a != b

if "aa" == "aa"
    error("String == did not dispatch to the user-defined method")
end

if !("aa" != "aa")
    error("String != did not route through user-defined ==")
end

if same_4298("aa", "aa")
    error("Any-typed String == did not dispatch to the user-defined method")
end

if !notsame_4298("aa", "aa")
    error("Any-typed String != did not route through user-defined ==")
end

if :aa == :aa
    error("Symbol == did not dispatch to the user-defined method")
end

if !(:aa != :aa)
    error("Symbol != did not route through user-defined ==")
end

if same_4298(:aa, :aa)
    error("Any-typed Symbol == did not dispatch to the user-defined method")
end

if !notsame_4298(:aa, :aa)
    error("Any-typed Symbol != did not route through user-defined ==")
end

if true == true
    error("Bool == did not dispatch to the user-defined method")
end

if !(true != true)
    error("Bool != did not route through user-defined ==")
end

if same_4298(true, true)
    error("Any-typed Bool == did not dispatch to the user-defined method")
end

if !notsame_4298(true, true)
    error("Any-typed Bool != did not route through user-defined ==")
end

if 'a' == 'a'
    error("Char == did not dispatch to the user-defined method")
end

if !('a' != 'a')
    error("Char != did not route through user-defined ==")
end

if same_4298('a', 'a')
    error("Any-typed Char == did not dispatch to the user-defined method")
end

if !notsame_4298('a', 'a')
    error("Any-typed Char != did not route through user-defined ==")
end

if Int64 == Int64
    error("Type{Int64} == did not dispatch to the user-defined method")
end

if !(Int64 != Int64)
    error("Type{Int64} != did not route through user-defined ==")
end

if same_4298(Int64, Int64)
    error("Any-typed Type{Int64} == did not dispatch to the user-defined method")
end

if !notsame_4298(Int64, Int64)
    error("Any-typed Type{Int64} != did not route through user-defined ==")
end

true
