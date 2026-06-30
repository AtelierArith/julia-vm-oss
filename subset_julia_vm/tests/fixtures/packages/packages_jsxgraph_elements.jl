using JSXGraph
b = board()
p1 = point(0, 0; name="O")
p2 = point(1, 0; name="X")
p3 = point(0, 1; name="Y")
l = line(p1, p2)
s = segment(p1, p3)
c = circle(p1, 1.0)
poly = polygon(p1, p2, p3)
t = text(1, 1, "hello")
push!(b, p1, p2, p3, l, s, c, poly, t)
ids = [e.id for e in b.elements]
length(ids) == 8 && ids == collect(1:8) &&
    b.elements[4].type_name == :line &&
    b.elements[5].type_name == :segment &&
    b.elements[6].type_name == :circle &&
    b.elements[7].type_name == :polygon &&
    b.elements[8].type_name == :text
