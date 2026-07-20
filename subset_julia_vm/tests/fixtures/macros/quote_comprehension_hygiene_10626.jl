# Issue #10626 (macro hygiene completeness, follow-up to #10253/#10242): a
# user-defined macro's `quote` body may return a comprehension or generator
# expression. Before this fix, sjulia's macro-expansion value->IR conversion
# had NO arm for Expr(:comprehension, ...) / Expr(:generator, ...) at all --
# any macro whose quote body contained one hard-errored with
# "macro expansion returned unsupported Expr head :comprehension" even though
# upstream Julia expands it fine. This is the RED->GREEN regression: the
# comprehension/generator now converts to the same Expr::Comprehension /
# Expr::Generator IR a non-quoted comprehension/generator produces, and its
# iteration variable participates in the same hygiene-rename pass as
# `local`/assignment targets/`catch` variables (Issue #10242) so it does not
# leak into the caller's scope under its literal name.
#
# Known limitation (tracked by Issue #10903, a `bug` filed alongside this
# fix): a `for`-loop or comprehension iteration variable that shadows a
# PRE-EXISTING same-named local in the *same* enclosing scope currently leaks
# into / overwrites that outer variable instead of properly shadowing it -- a
# general scoping defect in sjulia's `for`/comprehension lowering, not a
# hygiene gap (confirmed via `@macroexpand`: upstream Julia's own hygiene
# renames such colliding names to the *same* gensym too, relying on
# `for`/comprehension scoping -- not name-distinctness -- for isolation).
# This fixture therefore does not assert a same-name-collision scenario for
# the comprehension's own iteration variable against a SIBLING binding
# introduced by the same macro; it verifies that a comprehension/generator
# works at all inside a macro quote, that its loop variable does not leak
# into a caller-scope variable of the same name declared OUTSIDE the macro,
# and that `esc()` still preserves caller bindings referenced from inside it.
#
# #10903 also has a macro-hygiene-specific symptom (see the issue comment
# added when this fixture was written): if a comprehension/generator's own
# iteration variable shares a literal name with an unrelated SIBLING
# reference inside the *same* macro quote body (e.g. `(sum([sort for sort in
# 1:3]), sort([3, 1, 2]))`), the flat hygiene rename maps both occurrences to
# the same gensym (this part matches upstream, confirmed via
# `@macroexpand`), but because sjulia's loop variable then leaks past the end
# of the comprehension instead of going out of scope (#10903's core bug), the
# sibling reference resolves to a stale leftover value instead of upstream's
# `UndefVarError`. Both engines error on that construct today (so it is not a
# silent-wrong-output regression), just with a different error class; it is
# not asserted here and will converge automatically once #10903 is fixed.

macro squares(n)
    quote
        [x^2 for x in 1:$(esc(n))]
    end
end

result_basic = @squares(3)
check_basic = result_basic == [1, 4, 9]

# The macro's own internal comprehension var `x` must not leak into the
# caller's scope under its literal name.
x = "caller value"
result_again = @squares(4)
check_no_leak = x == "caller value"

# esc()'d references inside the comprehension body must still resolve in the
# caller's scope, distinct from the macro's own (hygiene-renamed) loop
# variable -- even when they share the exact same literal name `v`.
macro double_all(xs)
    quote
        [v * 2 for v in $(esc(xs))]
    end
end

v = [10, 20, 30]
result_esc = @double_all(v)
check_esc = result_esc == [20, 40, 60]

# Bare (lazy) generator expression inside a macro quote, consumed by sum().
macro sum_squares(n)
    quote
        sum(x^2 for x in 1:$(esc(n)))
    end
end
check_generator = @sum_squares(3) == 14

check_basic && check_no_leak && check_esc && check_generator
