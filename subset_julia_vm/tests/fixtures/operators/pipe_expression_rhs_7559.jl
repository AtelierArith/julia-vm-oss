function choose_pipe(x, choose)
    x |> (choose ? identity : string)
end

choose_pipe(7, true) == 7 || error("pipe selected identity branch incorrectly")
choose_pipe(7, false) == "7" || error("pipe selected string branch incorrectly")

true
