# Public String/Char migration names remain usable as ordinary function values.

string_fn = string
if string_fn("id=", 42, ", ok=", true) != "id=42, ok=true"
    error("function-valued string vararg call returned the wrong result")
end

if string_fn(Int32(-255); base=16, pad=4) != "-00ff"
    error("function-valued string integer base/pad call returned the wrong result")
end

if string_fn(UInt128(1) << 100; base=16) != "10000000000000000000000000"
    error("function-valued string UInt128 base call returned the wrong result")
end

if string_fn(true) != "true" || string_fn(false) != "false"
    error("function-valued string Bool call returned the wrong result")
end

if string_fn(true; base=2, pad=4) != "0001"
    error("function-valued string Bool base/pad call returned the wrong result")
end

if string_fn(BigFloat("0")) != "0.0" || string_fn(fld(BigFloat("1.0"), BigFloat(Inf))) != "0.0"
    error("function-valued string BigFloat call did not preserve Julia-compatible decimal output")
end

string_type_fn = String
chars = ['J', 'λ']
if string_type_fn(chars) != "Jλ"
    error("function-valued String(::Vector{Char}) call returned the wrong result")
end

bytes = UInt8[0x48, 0x69]
if string_type_fn(bytes) != "Hi"
    error("function-valued String(::Vector{UInt8}) call returned the wrong result")
end

if String(:foo) != "foo" || string_type_fn(Symbol("##x#363")) != "##x#363"
    error("String(::Symbol) did not return the bare symbol name")
end

char_fn = Char
if char_fn(0x03BB) != 'λ'
    error("function-valued Char(::Integer) call returned the wrong result")
end

int_fn = Int
if int_fn('λ') != 0x03BB
    error("function-valued Int(::Char) call returned the wrong result")
end

codeunits_fn = codeunits
cu = codeunits_fn("Aλ")
if length(cu) != 3
    error("function-valued codeunits length returned the wrong result")
end
if Int(cu[1]) != 0x41 || Int(cu[2]) != 0xCE || Int(cu[3]) != 0xBB
    error("function-valued codeunits indexing returned the wrong bytes")
end
if string_type_fn(cu) != "Aλ"
    error("String(::CodeUnits) did not recover the original string")
end

true
