# Keep the declaration well past byte zero: source offsets are local to this
# file and therefore cannot be compared with the later include's offsets.
struct IncludedInnerThenOuter11028
    x::Int
    IncludedInnerThenOuter11028(x::Int) = new(x + 1)
end
