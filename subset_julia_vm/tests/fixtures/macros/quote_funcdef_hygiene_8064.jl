# Issue #8064: a non-esc function name defined inside a macro's bare `quote` is
# hygienic — gensym'd / module-private — so it is NOT visible after the
# macrocall (upstream Julia raises `UndefVarError`), and two macros that share an
# internal helper name keep independent method tables instead of merging into one
# shared top-level binding.

# Two macros with the same internal helper name must not collide: each call sees
# its own definition.
macro hyg_a()
    quote
        hyg_helper() = 1
        hyg_helper()
    end
end
macro hyg_b()
    quote
        hyg_helper() = 2
        hyg_helper()
    end
end
check_no_collision = (@hyg_a() == 1) && (@hyg_b() == 2)

# A non-esc short-form name is not callable after the macrocall.
macro hyg_short()
    quote
        hyg_short_fn(x) = x + 1
    end
end
@hyg_short
check_short_hidden = try
    hyg_short_fn(1)
    false
catch
    true
end

# Same for the full `function ... end` form.
macro hyg_full()
    quote
        function hyg_full_fn(x)
            x + 1
        end
    end
end
@hyg_full
check_full_hidden = try
    hyg_full_fn(1)
    false
catch
    true
end

check_no_collision && check_short_hidden && check_full_hidden
