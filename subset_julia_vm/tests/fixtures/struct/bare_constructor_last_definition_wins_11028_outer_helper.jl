# Keep the ordinary definition well past byte zero: source offsets are local
# to this file and therefore cannot be compared with the later include.
IncludedOuterThenInner11028(x::Int) = :outer
