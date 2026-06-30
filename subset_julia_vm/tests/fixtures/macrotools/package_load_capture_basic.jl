using MacroTools: @capture

ex = :(f(42))
ok = @capture(ex, f(x_))
ok && x == 42
