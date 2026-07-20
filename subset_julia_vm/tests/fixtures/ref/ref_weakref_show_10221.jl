using Test

# Issue #10221: WeakRef display must show the referent with Julia show
# formatting, not Rust's RefCell Debug representation.

wr_int = WeakRef(42)
@test string(wr_int) == "WeakRef(42)"
@test sprint(show, wr_int) == "WeakRef(42)"

io_int = IOBuffer()
show(io_int, wr_int)
@test String(take!(io_int)) == "WeakRef(42)"

rooted_string_10221 = "hi"
wr_string = WeakRef(rooted_string_10221)
@test string(wr_string) == "WeakRef(\"hi\")"
@test sprint(show, wr_string) == "WeakRef(\"hi\")"

wr_nothing = WeakRef(nothing)
@test string(wr_nothing) == "WeakRef(nothing)"
@test sprint(show, wr_nothing) == "WeakRef(nothing)"

true
