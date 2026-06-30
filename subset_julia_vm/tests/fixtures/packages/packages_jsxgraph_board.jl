using JSXGraph
b = board("box"; xlim=(-5, 5), ylim=(-5, 5))
length(b.elements) == 0 && b.options[1].first == :boundingbox
