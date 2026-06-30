using JSXGraph
b = board("box"; xlim=(-5, 5), ylim=(-5, 5))
A = point(1, 2; name="A")
B = point(-3, -1; name="B")
l = line(A, B)
push!(b, A, B, l)
length(b.elements) == 3 && b.elements[1].type_name == :point && b.elements[3].type_name == :line
