using JSXGraph
c = functiongraph(sin; a=0, b=pi, n=5)
c.type_name == :curve && length(c.parents[1]) == 5 && length(c.parents[2]) == 5
