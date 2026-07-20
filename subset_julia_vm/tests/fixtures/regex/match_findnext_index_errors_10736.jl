# Issue #10736: out-of-range / negative indices for 3-arg match and regex
# findnext must error like upstream instead of returning nothing.
# - match(re, s, start) with start > ncodeunits(s) + 1 → ErrorException
#   ("PCRE.exec error: bad offset value")
# - findnext(re, s, i) with i < 1 → InexactError (upstream converts the
#   0-based offset i-1 to UInt)
# Workaround: regex literals are hoisted to variables because an r"..." inside
# a macro call argument fails lowering (Issue #11753).

x_pat = r"x"
d_pat = r"\d"
b_pat = r"b"
c_pat = r"c"

# match past-end offset: ErrorException with the upstream PCRE message.
match_past_end = try
    match(x_pat, "abc", 5)
    "no error"
catch e
    e
end
@assert match_past_end isa ErrorException

# findnext non-positive index: InexactError from the UInt offset conversion.
findnext_zero = try
    findnext(d_pat, "abc", 0)
    "no error"
catch e
    e
end
@assert findnext_zero isa InexactError

findnext_negative = try
    findnext(d_pat, "abc", -3)
    "no error"
catch e
    e
end
@assert findnext_negative isa InexactError

# Boundary cases stay valid: start == ncodeunits(s) + 1 is a legal offset
# (returns nothing on no match), and in-range searches still work.
@assert match(x_pat, "abc", 4) === nothing
@assert match(c_pat, "abc", 3).match == "c"
@assert findnext(b_pat, "abc", 1) == 2:2
@assert findnext(b_pat, "abc", 3) === nothing

# findnext past-end keeps its existing BoundsError.
findnext_past_end = try
    findnext(b_pat, "abc", 5)
    "no error"
catch e
    e
end
@assert findnext_past_end isa BoundsError

println("All match/findnext index error tests passed")
true
