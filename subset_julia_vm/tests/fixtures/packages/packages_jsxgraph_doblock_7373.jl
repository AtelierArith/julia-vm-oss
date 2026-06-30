using JSXGraph

b = board(; xlim=(-2, 2), ylim=(-2, 2)) do board_ref
    push!(board_ref, point(0, 0; name="O"))
end

length(b.elements) == 1 &&
    b.elements[1].type_name == :point &&
    b.elements[1].attrs[1].first == :name
