macro macro_expanded_pair_7639()
    esc(:(Dict(:a => 1, :b => 2)))
end

d = @macro_expanded_pair_7639

d[:a] == 1 && d[:b] == 2
