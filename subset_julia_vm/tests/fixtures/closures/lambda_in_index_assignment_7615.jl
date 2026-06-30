function closures_lambda_in_index_assignment_7615()
    xs = [[1]]
    xs[1] = map(x -> x + 1, xs[1])
    xs[1][1] == 2
end

closures_lambda_in_index_assignment_7615()
