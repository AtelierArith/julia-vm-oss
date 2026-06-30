using Plots

function run_plot_push(n::Int64)
    plt = plot3d(1)

    i = 1
    while i <= n
        x = 0.001 * i
        y = 0.002 * i
        z = 0.003 * i
        push!(plt, x, y, z)
        i += 1
    end

    return length(plt.series[1].x)
end

println(run_plot_push(3000))
