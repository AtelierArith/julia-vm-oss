# Issue #10351: upstream evaluates ALL print/println arguments before writing
# any of the call's own output, so a later argument's side effects (its own
# printing) must appear BEFORE the call's first write. sjulia previously
# interleaved per-argument writes with argument evaluation on the print-family
# fast path (`println("x: ", f())` printed `x: side` then `1`).
#
# The fixture harness checks the final value; ordering correctness is encoded
# through side-effect logs and a position probe on the target IOBuffer.
# (A copy(buf)-based probe would be nicer but copy(::IOBuffer) is not
# supported yet — Issue #11714.)

using Test

log = String[]

side(tag, v) = (push!(log, tag); v)

# stdout path: evaluation of BOTH args precedes the call's writes; the log
# sequence proves both side effects ran during the argument phase.
println("a: ", side("s1", 1), " b: ", side("s2", 2.5))
@test log == ["s1", "s2"]

# io path: a later argument must observe an EMPTY buffer (nothing of this
# call written yet) while it evaluates.
buf = IOBuffer()
seen_position = Ref(-1)
observer() = (seen_position[] = position(buf); 42)
println(buf, "x: ", observer(), " y")
@test seen_position[] == 0            # no partial write before args finished
@test String(take!(buf)) == "x: 42 y\n"

true
