using Test

new(; x) = x
keyword_ownerless_new11204() = new(x = 7)
keyword_splat_ownerless_new11204(opts) = new(; opts...)

@test keyword_ownerless_new11204() == 7
@test keyword_splat_ownerless_new11204((x = 8,)) == 8

true
