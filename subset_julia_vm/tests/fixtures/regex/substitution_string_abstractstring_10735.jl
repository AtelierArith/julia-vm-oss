# Issue #10735: SubstitutionString must behave as an AbstractString outside
# `replace` (upstream base/regex.jl forwards the AbstractString interface to
# the wrapped string). Previously `s"abc" == "abc"` was false and
# `length(s"abc")` raised MethodError — and porting the upstream methods
# exposed a VM dispatch-miss gap for builtin-backed names (`ncodeunits`,
# `codeunit`) fixed alongside in call_dynamic.rs.

s = s"abc"

# Equality with plain String, both directions, and between SubstitutionStrings.
# The SubstitutionString operands are hoisted (Issue #11756: an s"..." inside a
# macro call argument degrades to a plain String, which would silently test the
# String `==` method instead).
same_sub = s"abc"
other_sub = s"xyz"
@assert (s == "abc") == true
@assert ("abc" == s) == true
@assert (s == "abd") == false
@assert (s == same_sub) == true
@assert (s == other_sub) == false

# AbstractString surface.
@assert length(s) == 3
@assert ncodeunits(s) == 3
@assert codeunit(s, 1) == UInt8(97)
@assert isvalid(s, 1) == true
@assert s[1] == 'a'
@assert s[2:3] == "bc"
@assert collect(s) == ['a', 'b', 'c']
@assert eltype(s) == Char
@assert String(s) == "abc"
@assert hash(s) == hash("abc")

# Multi-byte content forwards byte-accurate ncodeunits/length.
m = s"aé"
@assert length(m) == 2
@assert ncodeunits(m) == 3

# The replace machinery is unperturbed: plain replacement, regex with
# SubstitutionString capture expansion, and plain-String literal replacement.
# Workaround: regex and s"..." literals are hoisted to variables because inside
# a macro call argument an r"..." fails lowering (Issue #11753) and an s"..."
# silently degrades to a plain String (Issue #11756).
word_pat = r"(w\w+)"
x_pat = r"x"
word_sub = s"<\1>"
@assert replace("aaa", "a" => "b") == "bbb"
@assert replace("hello world", word_pat => word_sub) == "hello <world>"
@assert replace("axc", x_pat => "y") == "ayc"

# show/print keep their s"..." / plain-content split.
# Workaround: the expected repr value is hoisted because an escaped-quote
# string literal inside a macro call argument is mangled (Issue #11757).
expected_repr = "s\"abc\""
@assert repr(s) == expected_repr
# Workaround: `string(s)` is compared through a temp variable because a direct
# `string(s) == "abc"` comparison hits a Str-typed `==` fast path that misfires
# on the non-String AbstractString result (Issue #11755).
string_s = string(s)
@assert string_s == "abc"
@assert string_s isa SubstitutionString

# Builtin fallback on dispatch miss: ncodeunits/codeunit on plain String still
# work while the SubstitutionString methods are defined (the #10735 VM gap).
@assert ncodeunits("hello") == 5
@assert codeunit("hello", 1) == UInt8(104)

# Regression: a Bottom/Any-inferred operand (last of a heterogeneous tuple,
# Issue #11765) must not be statically bound to ==(::SubstitutionString,
# ::String) — plain String equality goes through runtime dispatch instead
# (static_binary_match_binds_unknown_to_struct guard).
het = (1, 2.0, "hello")
@assert last(het) == "hello"
@assert (s"hello",)[1] == "hello"

println("All SubstitutionString AbstractString tests passed")
true
