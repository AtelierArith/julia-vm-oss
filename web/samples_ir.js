// IR JSON for web playground samples
// Derived from the iOS app sample library
// (SubsetJuliaVMApp/.../Resources/Samples). Generated to mirror samples.json:
// same entries, same order. Keep in sync when iOS samples change.
//
// All bundled samples (Primes/Symbolics/Distributions packages, JSXGraph boards,
// Plots/Interact figures) run end-to-end in the static WASM build, so every entry
// has webUnsupported: false (Issue #7286 / #7310). Set the flag true only if a
// sample genuinely cannot run in the web build.

export const samplesIR = [
  {
    id: "hello_world",
    name: "Hello World",
    category: "Basic",
    description: "The classic first program - print a greeting to the console",
    difficulty: "Beginner",
    tags: ["print","string"],
    folder: "beginner",
    code: `#=
==========================================
Welcome to SubsetJuliaVM!
==========================================

HOW TO USE THIS APP:
1. Select a sample from the dropdown menu above
2. Edit the Julia code in this editor
3. Tap "Run" to execute the code
4. View results in the Output section below
5. Drag the divider between editor and output
   to resize both areas as needed

TIP: The divider handle (gray bar) can be
dragged up/down to adjust the split ratio.

==========================================
SubsetJuliaVM へようこそ！
==========================================

アプリの使い方:
1. 上のドロップダウンからサンプルを選択
2. このエディタで Julia コードを編集
3. 「Run」ボタンをタップしてコードを実行
4. 下の Output セクションで結果を確認
5. エディタと出力の間のディバイダーを
   ドラッグして両領域のサイズを調整可能

ヒント: 灰色のバー（ディバイダーハンドル）を
上下にドラッグして分割比率を調整できます。
==========================================
=#

println("Hello, World!")
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "memo",
    name: "Memo (自由帳)",
    category: "Basic",
    description: "Use this space for your own code experiments",
    difficulty: "Beginner",
    tags: ["memo","scratchpad"],
    folder: "beginner",
    code: ``,
    ir: null,
    webUnsupported: false
  },
  {
    id: "fizzbuzz",
    name: "FizzBuzz",
    category: "Algorithms",
    description: "Classic FizzBuzz problem with if/elseif/else",
    difficulty: "Intermediate",
    tags: ["fizzbuzz","if","elseif","else","for","modulo"],
    folder: "intermediate",
    code: `function fizzbuzz(n)
    for i in 1:n
        if i % 15 == 0
            println("FizzBuzz")
        elseif i % 3 == 0
            println("Fizz")
        elseif i % 5 == 0
            println("Buzz")
        else
            println(i)
        end
    end
end

fizzbuzz(100)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "matrix_multiplication",
    name: "Matrix Multiplication",
    category: "Arrays",
    description: "Matrix-vector and matrix-matrix multiplication",
    difficulty: "Intermediate",
    tags: ["array","matrix","vector","multiplication","linear-algebra"],
    folder: "intermediate",
    code: `# Matrix-Vector Multiplication
A = zeros(2, 3)
A[1, 1] = 1; A[1, 2] = 2; A[1, 3] = 3
A[2, 1] = 4; A[2, 2] = 5; A[2, 3] = 6

v = [1, 2, 3]
result_v = A * v
println("Matrix A (2x3) * vector v:")
println("  result[1] = ", result_v[1], " (expected 14)")
println("  result[2] = ", result_v[2], " (expected 32)")

# Matrix-Matrix Multiplication
B = zeros(3, 2)
B[1, 1] = 7; B[1, 2] = 8
B[2, 1] = 9; B[2, 2] = 10
B[3, 1] = 11; B[3, 2] = 12

C = A * B  # Result is 2x2
println("Matrix A (2x3) * Matrix B (3x2):")
println("  C[1,1] = ", C[1, 1], " (expected 58)")
println("  C[1,2] = ", C[1, 2], " (expected 64)")
println("  C[2,1] = ", C[2, 1], " (expected 139)")
println("  C[2,2] = ", C[2, 2], " (expected 154)")

println(C[2, 2])
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "plotting_2d",
    name: "Plotting (2D)",
    category: "Visualization",
    description: "Plot functions with plot/plot!, then overlay markers and bars!",
    difficulty: "Intermediate",
    tags: ["plots","plot","scatter","bar","visualization","broadcast","sin","cos"],
    folder: "intermediate",
    code: `# 2D plotting with Plots.jl — plots render interactively via Plotly.
using Plots

# plot(f) draws a function over its default domain.
plot(sin)

# plot! mutates the current plot, overlaying another series.
plot!(cos)

# Broadcast over a range to build coordinates, then scatter! adds markers.
t = 0:0.1:2π
scatter!(cos.(t), sin.(t), aspect_ratio=:equal)

# bar! overlays a bar trace on the same Plotly figure.
bar!([1, 2, 3], [0.4, 0.8, 0.6])
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "plotting_3d",
    name: "Plotting (3D)",
    category: "Visualization",
    description: "Draw a 3D helix with plot!(x,y,z), then overlay 3D markers with scatter!",
    difficulty: "Intermediate",
    tags: ["plots","plot","scatter","visualization","3d","helix","broadcast"],
    folder: "intermediate",
    code: `# 3D plotting with Plots.jl — 3D plots render interactively via Plotly.
using Plots

# A helix: plot!(x, y, z) draws a 3D line (path3d).
# With no current plot, plot! starts a new one.
t = 0:0.1:2π
plot!(cos.(t), sin.(t), t)

# scatter!(x, y, z) overlays 3D markers (scatter3d) on the same axes.
t = 0:0.5:2π
scatter!(cos.(t), sin.(t), t)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "ordinarydiffeq_linear_ode",
    name: "OrdinaryDiffEq Linear ODE",
    category: "Visualization",
    description: "Solve the OrdinaryDiffEq README linear ODE sample and overlay the analytical solution",
    difficulty: "Intermediate",
    tags: ["ordinarydiffeq","ode","plots","plotly","rk4","package"],
    folder: "intermediate",
    code: `# OrdinaryDiffEq README linear ODE sample, rendered with Plots/Plotly.
using OrdinaryDiffEq

f(u, p, t) = 1.01 * u
u0 = 1 / 2
tspan = (0.0, 1.0)
prob = ODEProblem(f, u0, tspan)
sol = solve(prob, Tsit5(), dt=0.1, reltol=1e-8, abstol=1e-8)

using Plots
plot(sol, linewidth=5, title="Solution to the linear ODE with a thick line",
     xaxis="Time (t)", yaxis="u(t)", label="My Thick Line!")
plot!(sol.t, t -> 0.5 * exp(1.01 * t), lw=3, ls=:dash, label="True Solution!")
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "ordinarydiffeq_lorenz_attractor",
    name: "OrdinaryDiffEq Lorenz Attractor",
    category: "Visualization",
    description: "Solve the OrdinaryDiffEq README Lorenz system and render the trajectory as a 3D Plotly path",
    difficulty: "Intermediate",
    tags: ["ordinarydiffeq","lorenz","ode","plots","plotly","3d","package"],
    folder: "intermediate",
    code: `# OrdinaryDiffEq README Lorenz sample, rendered as a 3D Plotly path.
using OrdinaryDiffEq

function lorenz!(du, u, p, t)
    du[1] = 10.0 * (u[2] - u[1])
    du[2] = u[1] * (28.0 - u[3]) - u[2]
    du[3] = u[1] * u[2] - (8 / 3) * u[3]
end

u0 = [1.0, 0.0, 0.0]
tspan = (0.0, 60.0)
prob = ODEProblem(lorenz!, u0, tspan)
sol = solve(prob, Tsit5(), dt=0.02, saveat=0.02)

using Plots
plot(sol, idxs=(1, 2, 3), title="Lorenz attractor")
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "plots_torus",
    name: "Torus (Plots.jl)",
    category: "Visualization",
    description: "Draw a torus as a wireframe of meridian and longitude rings with plot3d/plot3d!",
    difficulty: "Intermediate",
    tags: ["plots","plot3d","torus","wireframe","3d","parametric","visualization"],
    folder: "intermediate",
    code: `# Torus visualization with Plots.jl — rendered interactively via Plotly.
# A torus is a closed surface, so it cannot be written as z = f(x, y).
# Instead we draw it as a wireframe: rings around the tube (meridians) plus
# rings around the hole (longitudes), overlaid with plot3d!.
using Plots

R = 2.0   # distance from the center of the tube to the center of the torus
r = 0.7   # radius of the tube

# Parametric torus:
#   x = (R + r*cos(v)) * cos(u)
#   y = (R + r*cos(v)) * sin(u)
#   z =  r*sin(v)
# u sweeps around the hole, v sweeps around the tube.

# Bind the ranges to variables before the for-heads: an inline integer-start,
# float-step range in a for-head currently iterates zero times (Issue #7800),
# so we iterate over bound range variables instead.
us = 0:(2π/24):2π      # samples around the hole
vs = 0:(2π/24):2π      # samples around the tube
uring = 0:(2π/12):2π   # 13 meridian rings
vring = 0:(2π/12):2π   # 13 longitude rings

# Meridian rings: fix u, sweep v (small circles around the tube).
first = true
for u in uring
    x = (R .+ r .* cos.(vs)) .* cos(u)
    y = (R .+ r .* cos.(vs)) .* sin(u)
    z = r .* sin.(vs)
    if first
        plot3d(x, y, z; title="Torus (Plots.jl)")
        global first = false
    else
        plot3d!(x, y, z)
    end
end

# Longitude rings: fix v, sweep u (large circles around the hole).
for v in vring
    x = (R + r * cos(v)) .* cos.(us)
    y = (R + r * cos(v)) .* sin.(us)
    z = fill(r * sin(v), length(us))
    plot3d!(x, y, z)
end

# Return the assembled figure so the host renders it.
current()
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "sinc_surface",
    name: "Sinc Surface",
    category: "Visualization",
    description: "Render a 3D sinc surface from a function-valued z argument",
    difficulty: "Intermediate",
    tags: ["plots","surface","visualization","3d","sinc","linear-algebra"],
    folder: "intermediate",
    code: `# Surface plot from a function-valued z argument.
using Plots
using LinearAlgebra

x = y = range(-3, stop = 3, length = 100)
surface(x, y, (x, y) -> sinc(norm([x, y])))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "plots_animation",
    name: "Animation (@gif)",
    category: "Visualization",
    description: "Build a looping Plotly animation with @gif / @animate — a fresh titled plot per frame",
    difficulty: "Intermediate",
    tags: ["plots","animation","gif","animate","visualization","macro"],
    folder: "intermediate",
    code: `# Animations with Plots.jl — @gif renders an auto-playing, looping Plotly
# animation (Play/Pause + a frame slider). Each loop iteration builds a fresh
# plot; title="t=\$t" labels every frame, so the title updates as it plays.
using Plots

x = -π:0.01:π
ps = []
@gif for t in -π:0.1:π
    p = plot(x, sin.(x .- t), title="t=\$t")
    push!(ps, p)
end

# \`ps\` now holds every frame's Plot, so you can replay a list of pre-built
# plots with another @gif (plot(p) re-selects an existing Plot):
#
#   @gif for p in ps
#       plot(p)
#   end
#
# The split form is equivalent: @animate collects the frames into an
# Animation, and gif(anim) turns it into the playable artifact.
#
#   anim = @animate for t = -π:0.1:π
#       plot(x, sin.(x .- t), title="t=\$t")
#   end
#   gif(anim)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "interact_manipulate",
    name: "Interactive dropdown (@manipulate)",
    category: "Visualization",
    description: "Switch between per-choice plots with Interact's @manipulate — a static Plotly figure with a dropdown",
    difficulty: "Intermediate",
    tags: ["interact","manipulate","plots","dropdown","visualization","macro"],
    folder: "intermediate",
    code: `# Interact.jl's @manipulate (MVP) — evaluate the body once per discrete choice
# and combine the per-choice plots into ONE static Plotly figure with a dropdown
# (updatemenus) that switches between them. No reactive runtime needed.
using Interact, Plots

datasets = Dict(
    :squares => [1.0, 4.0, 9.0, 16.0, 25.0],
    :primes  => [2.0, 3.0, 5.0, 7.0, 11.0],
)

@manipulate for dataset = [:squares, :primes]
    scatter(datasets[dataset])
end
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "interact_sine_frequency",
    name: "Interactive sine frequency (@manipulate)",
    category: "Visualization",
    description: "Sweep an integer frequency k = 1:5 with Interact's @manipulate over a numeric range — a static Plotly figure with a value-labelled dropdown",
    difficulty: "Intermediate",
    tags: ["interact","manipulate","plots","dropdown","range","sin","visualization","macro"],
    folder: "intermediate",
    code: `# Interact.jl's @manipulate over a numeric range (MVP) — sweep an integer
# frequency k = 1:5 and watch sin(k·x) change. The body is evaluated once per
# discrete choice and the per-choice plots are combined into ONE static Plotly
# figure with a dropdown (updatemenus) labelled by value ("1".."5"). Unlike the
# Dict/Symbol dropdown sample, here the choices come from a range, so each label
# is the numeric value of k. (sjulia MVP renders every @manipulate control as a
# dropdown — continuous sliders are out of scope; see Issue #7275.)
using Interact, Plots

xs = range(0, 2π, length=200)

@manipulate for k = 1:5
    plot(xs, sin.(k .* xs), title="sin(k·x), k=\$k")
end
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "aizawa_attractor",
    name: "Aizawa Attractor",
    category: "Visualization",
    description: "Animate the Aizawa 3D strange attractor with plot3d, push!(plt, x, y, z), and @animate ... every N",
    difficulty: "Intermediate",
    tags: ["plots","animation","animate","gif","3d","attractor","struct","kwdef","visualization"],
    folder: "intermediate",
    code: `# Aizawa attractor — a 3D strange attractor animated with Plots.jl.
# A mutable struct holds the state and parameters; step! advances the orbit one
# Euler step. plot3d(1) starts an empty 3D path; push!(plt, x, y, z) appends each
# new point, and @animate ... every 20 samples one frame per 20 steps to build a
# looping Plotly animation (Play/Pause + a frame slider).
using Plots

Base.@kwdef mutable struct Aizawa
    dt::Float64 = 0.01
    a::Float64 = 0.95
    b::Float64 = 0.7
    c::Float64 = 0.6
    d::Float64 = 3.5
    e::Float64 = 0.25
    f::Float64 = 0.1
    x::Float64 = 0.1
    y::Float64 = 0.0
    z::Float64 = 0.0
end

function step!(s::Aizawa)
    dx = (s.z - s.b) * s.x - s.d * s.y
    dy = s.d * s.x + (s.z - s.b) * s.y
    dz = s.c + s.a * s.z - s.z^3 / 3 - (s.x^2 + s.y^2) * (1 + s.e * s.z) + s.f * s.z * s.x^3
    s.x = s.x + s.dt * dx
    s.y = s.y + s.dt * dy
    s.z = s.z + s.dt * dz
    return s
end

attractor = Aizawa()
plt = plot3d(1, xlim=(-1.5,1.5), ylim=(-1.5,1.5), zlim=(-0.5,1.7),
             title="Aizawa Attractor", legend=false, marker=2)
anim = @animate for i in 1:3000
    step!(attractor)
    push!(plt, attractor.x, attractor.y, attractor.z)
end every 20
gif(anim)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "ifs_fractals",
    name: "Interactive fractals (@manipulate)",
    category: "Visualization",
    description: "Pick an iterated function system (Barnsley fern, Sierpinski triangle, Heighway dragon) from a dropdown and draw it with the chaos game — Interact's @manipulate combines the per-choice scatter plots into one Plotly figure",
    difficulty: "Intermediate",
    tags: ["interact","manipulate","dropdown","plot","scatter","fractal","ifs","random","distributions","categorical"],
    folder: "intermediate",
    code: `# Interactive fractal explorer — pick an iterated function system (IFS) from the
# dropdown and watch the chaos game fill out a different fractal. Each fractal is
# a set of affine maps x -> W*x + b applied at random; Interact.jl's @manipulate
# (MVP) evaluates the body once per choice and combines the per-choice scatter
# plots into ONE static Plotly figure with a dropdown.
using Interact, Plots, Distributions, Random

# Each affine map is x -> W*x + b on a 2-vector. We store the six coefficients
# as PLAIN SCALARS and apply them with scalar arithmetic (returning a tuple),
# instead of building 2x2 \`Matrix{Float64}\` / \`Vector{Float64}\` and calling the
# generic \`W * x + b\`. The chaos game runs this map 5000x per fractal; the
# generic small matrix*vector path allocates a fresh array each step and is ~10x
# slower in the VM (Issue #7949), so the scalar form cuts the whole sample from
# ~3.2s to ~0.8s with byte-identical output. This is an INTERIM hand-unroll: the
# clean fix is StaticArrays \`SMatrix*SVector\` once its arithmetic lands (Issue
# #7461). (a*x1 + b*x2 + e, c*x1 + d*x2 + f) is W*x + b written out.
struct Affine
    a::Float64
    b::Float64
    c::Float64
    d::Float64
    e::Float64
    f::Float64
end

(m::Affine)(x1, x2) = (m.a * x1 + m.b * x2 + m.e, m.c * x1 + m.d * x2 + m.f)

# Each fractal's maps and Categorical \`picker\` are bound DIRECTLY inside the
# @manipulate body's if/elseif (not looked up from a Dict/function): routing a
# Distribution through a function return or struct field currently breaks \`rand\`
# dispatch (Issue #7901), and a tuple \`a, b = f(x)\` inside a @manipulate body
# fails to lower (Issue #7900). Direct local bindings sidestep both and keep each
# fractal's definition in one readable place. For the same #7900 reason the loop
# reads the returned tuple via \`q[1]\`/\`q[2]\` rather than destructuring.
@manipulate for fractal = [:fern, :sierpinski, :dragon]
    Random.seed!(42)
    if fractal == :fern
        # Barnsley fern: coefficients (a, b, c, d, e, f) for x -> W*x + b.
        maps = (
            Affine(0.0, 0.0, 0.0, 0.16, 0.0, 0.0),
            Affine(0.85, 0.04, -0.04, 0.85, 0.0, 1.6),
            Affine(0.20, -0.26, 0.23, 0.22, 0.0, 1.6),
            Affine(-0.15, 0.28, 0.26, 0.24, 0.0, 0.44),
        )
        picker = Categorical([0.01, 0.85, 0.07, 0.07])
        ttl = "Barnsley Fern"
    elseif fractal == :sierpinski
        # Sierpinski triangle: shrink by 1/2 toward one of three vertices, equal odds.
        maps = (
            Affine(0.5, 0.0, 0.0, 0.5, 0.0, 0.0),
            Affine(0.5, 0.0, 0.0, 0.5, 1.0, 0.0),
            Affine(0.5, 0.0, 0.0, 0.5, 0.5, 0.866),
        )
        picker = Categorical([1/3, 1/3, 1/3])
        ttl = "Sierpinski Triangle"
    else
        # Heighway dragon: two rotate-and-scale maps, equal odds.
        maps = (
            Affine(0.5, -0.5, 0.5, 0.5, 0.0, 0.0),
            Affine(-0.5, -0.5, 0.5, -0.5, 1.0, 0.0),
        )
        picker = Categorical([0.5, 0.5])
        ttl = "Heighway Dragon"
    end
    n = 5000
    xs = zeros(n)
    ys = zeros(n)
    x1 = 0.0
    x2 = 0.0
    for i in 1:n
        idx = rand(picker)
        q = maps[idx](x1, x2)
        x1 = q[1]
        x2 = q[2]
        xs[i] = x1
        ys[i] = x2
    end
    scatter(xs, ys; aspect_ratio = :equal, title = ttl)
end
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "multiple_dispatch",
    name: "Multiple Dispatch",
    category: "Functions",
    description: "Julia's multiple dispatch: select method based on argument types",
    difficulty: "Intermediate",
    tags: ["dispatch","types","function","polymorphism"],
    folder: "intermediate",
    code: `# Multiple dispatch: same function name, different type signatures

# Method for integers
function process(x::Int64)
    println("Integer method: ", x, " → ", x * 2)
    return x * 2
end

# Method for floats
function process(x::Float64)
    println("Float method: ", x, " → ", x / 2.0)
    return x / 2.0
end

# Integer literals dispatch to Int64 method
r1 = process(42)

# Float literals dispatch to Float64 method
r2 = process(10.0)

println("Results: ", r1, ", ", r2)
println(r1)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "fibonacci",
    name: "Fibonacci",
    category: "Algorithms",
    description: "Fibonacci sequence: recursive vs iterative approaches",
    difficulty: "Intermediate",
    tags: ["recursion","fibonacci","dynamic-programming"],
    folder: "intermediate",
    code: `# Recursive (simple but slow for large n)
function fib_slow(n)
    if n <= 1
        return n
    end
    fib_slow(n - 1) + fib_slow(n - 2)
end

# Iterative (fast)
function fib_fast(n)
    if n <= 1
        return n
    end
    a = 0
    b = 1
    for i in 2:n
        c = a + b
        a = b
        b = c
    end
    b
end

println("Recursive fib(15) = ", fib_slow(15))
println("Iterative fib(30) = ", fib_fast(30))

println(fib_fast(30))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "is_prime",
    name: "Is Prime",
    category: "Algorithms",
    description: "Check if a number is prime",
    difficulty: "Intermediate",
    tags: ["for","if","prime","sqrt"],
    folder: "intermediate",
    code: `function is_prime(n)
    if n <= 1
        return 0
    end
    if n <= 3
        return 1
    end
    for i in 2:sqrt(n)
        if n % i == 0
            return 0
        end
    end
    1
end

# Test some numbers
println("is_prime(97) = ", is_prime(97))
println("is_prime(100) = ", is_prime(100))

# Count primes up to 50
count = 0
for i in 2:50
    if is_prime(i) == 1
        global count += 1
    end
end
println("Primes up to 50: ", count)

println(is_prime(97))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "structs",
    name: "Structs",
    category: "Structures",
    description: "Define immutable and mutable structs with typed fields",
    difficulty: "Intermediate",
    tags: ["struct","mutable","type","fields"],
    folder: "intermediate",
    code: `# Immutable struct
struct Point
    x::Float64
    y::Float64
end

# Mutable struct
mutable struct Counter
    value::Float64
end

# Functions that work with structs
function distance(p1, p2)
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    sqrt(dx*dx + dy*dy)
end

# Create and use
origin = Point(0.0, 0.0)
p = Point(3.0, 4.0)
println("Distance from origin: ", distance(origin, p))

# Mutable struct can be modified
c = Counter(0.0)
for i in 1:5
    c.value = c.value + 1.0
end
println("Counter value: ", c.value)

println(distance(origin, p))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "operator_overloading",
    name: "Operator Overloading",
    category: "Structures",
    description: "Extend +/- operators for custom struct types using multiple dispatch",
    difficulty: "Intermediate",
    tags: ["struct","operator","overloading","dispatch","point"],
    folder: "intermediate",
    code: `# Define a 2D Point struct
struct Point
    x::Float64
    y::Float64
end

# Overload the + operator for Point
function Base.:+(a::Point, b::Point)
    return Point(a.x + b.x, a.y + b.y)
end

# Overload the - operator for Point
function Base.:-(a::Point, b::Point)
    return Point(a.x - b.x, a.y - b.y)
end

# Create two points
p1 = Point(1.0, 2.0)
p2 = Point(3.0, 4.0)

# Use the overloaded operators
p3 = p1 + p2
println("p1 = (", p1.x, ", ", p1.y, ")")
println("p2 = (", p2.x, ", ", p2.y, ")")
println("p1 + p2 = (", p3.x, ", ", p3.y, ")")

p4 = p2 - p1
println("p2 - p1 = (", p4.x, ", ", p4.y, ")")

# Chain operations
p5 = p1 + p2 + Point(10.0, 10.0)
println("p1 + p2 + (10,10) = (", p5.x, ", ", p5.y, ")")

p3.x + p3.y
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "modules",
    name: "Modules",
    category: "Structures",
    description: "Group types and functions into a namespaced module, export names, and call them via using",
    difficulty: "Intermediate",
    tags: ["module","namespace","export","using","struct"],
    folder: "intermediate",
    code: `# Modules group related types and functions into their own namespace.
# \`export\` makes names available after \`using\`; others stay module-qualified.

module Geometry

export Point, distance, centroid

struct Point{T<:Real}
    x::T
    y::T
end

# Extend Base operators so Points add and subtract componentwise.
Base.:+(p::Point{T}, q::Point{T}) where {T<:Real} = Point(p.x + q.x, p.y + q.y)

distance(p::Point, q::Point) = sqrt((q.x - p.x)^2 + (q.y - p.y)^2)

function centroid(points::Vector{<:Point})
    n = length(points)
    sx = sum(p.x for p in points)
    sy = sum(p.y for p in points)
    return Point(sx / n, sy / n)
end

end # module Geometry

# Bring the exported names into scope. \`.Geometry\` is a submodule of Main.
using .Geometry

p = Point(3, 4)
q = Point(0, 0)

println("p + q   = ", p + q)
println("distance = ", distance(p, q))
println("centroid = ", centroid([Point(1, 2), Point(3, 4), Point(5, 6)]))

# Non-exported names are still reachable via the module path.
println("qualified call: ", Geometry.distance(Point(0, 0), Point(6, 8)))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "mandelbrot_heatmap",
    name: "Mandelbrot Heatmap",
    category: "Visualization",
    description: "Visualize the Mandelbrot set as a color heatmap using Plots.jl",
    difficulty: "Intermediate",
    tags: ["mandelbrot","fractal","heatmap","plots","visualization","broadcast","complex"],
    folder: "intermediate",
    code: `# Mandelbrot set visualized as a heatmap with Plots.jl.
using Plots

function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute a 2D escape-time grid via broadcasting.
function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymin, ymax; length=height)

    # xs' is a row vector, ys is a column vector → broadcasting builds a complex grid
    C = xs' .+ im .* ys

    # Ref(maxiter) prevents maxiter from being broadcast
    mandelbrot_escape.(C, Ref(maxiter))
end

# Render the escape-time grid as a heatmap.
@time grid = mandelbrot_grid(200, 150, 80)
heatmap(range(-2.0, 1.0; length=200), range(-1.2, 1.2; length=150), grid;
        title="Mandelbrot Set", aspect_ratio=:equal)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "jsxgraph_demo",
    name: "JSXGraph Demo",
    category: "Visualization",
    description: "Interactive geometry with JSXGraph.jl: points, segments, polygon, circle, function graph",
    difficulty: "Intermediate",
    tags: ["jsxgraph","geometry","visualization","interactive","functiongraph","polygon"],
    folder: "intermediate",
    code: `# Interactive geometry with JSXGraph.jl — points, segments, a polygon, a circle,
# a function graph, and a text label are composited on a board and rendered as
# an application/vnd.jsxgraph+json display artifact.
using JSXGraph

# keepAspectRatio=true keeps the x and y scales equal so circles look round.
b = board(; xlim=(-3, 3), ylim=(-3, 3), axis=true, grid=true, keepAspectRatio=true)

A = point(-2, -1; name="A", fillColor="red")
B = point(2, -1; name="B", fillColor="red")
C = point(0, 2; name="C", fillColor="red")

# Connect the points with segments
ab = segment(A, B)
bc = segment(B, C)
ca = segment(C, A)

# A filled polygon through the three points
tri = polygon(A, B, C; fillColor="lightblue")

# A circle centered at A with radius 2
circ = circle(A, 2.0; strokeColor="orange")

# Plot sin(x) over [-π, π] as a sampled curve
curve = functiongraph(sin; a=-π, b=π, strokeColor="green")

# Label in the upper-left corner
lbl = text(-2, 2.5, "JSXGraph demo")

push!(b, A, B, C, ab, bc, ca, tri, circ, curve, lbl)
html(b)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "apollonian_gasket",
    name: "Apollonian Gasket",
    category: "Visualization",
    description: "Apollonian circle packing from the Descartes Circle Theorem, rendered with JSXGraph.jl",
    difficulty: "Intermediate",
    tags: ["jsxgraph","apollonian","descartes","circle","fractal","geometry","complex","recursion"],
    folder: "intermediate",
    code: `# Apollonian circle packing (gasket) with JSXGraph.jl.
#
# Four mutually tangent circles satisfy the Descartes Circle Theorem, written in
# terms of curvatures (bends) bᵢ = 1/rᵢ:
#
#     (b₁ + b₂ + b₃ + b₄)² = 2(b₁² + b₂² + b₃² + b₄²)
#
# Lagarias–Mallows–Wilks extended it to the complex coordinates of the centers,
# letting us track wᵢ = bᵢ·zᵢ (z the center as a complex number). Replacing one
# circle of a Descartes configuration with the *other* tangent circle is then a
# pure linear "swap" (no square roots):
#
#     b₄′  = 2(b₁ + b₂ + b₃) − b₄
#     w₄′  = 2(w₁ + w₂ + w₃) − w₄
#
# Recursing that swap into every curvilinear triangle fills the disk with the
# self-similar Apollonian gasket. Reference: D. Austin, "When Kissing Involves
# Trigonometry", AMS Feature Column, March 2006.
using JSXGraph

# A circle stored as (bend, bend·center): the form in which the swap is linear.
struct Circ
    bend::Float64
    bz::Complex{Float64}
end

ccenter(c::Circ) = c.bz / c.bend
cradius(c::Circ) = 1.0 / abs(c.bend)

# The partner of c4 among the circles tangent to the mutually tangent c1,c2,c3.
function partner(c1::Circ, c2::Circ, c3::Circ, c4::Circ)
    nb = 2.0 * (c1.bend + c2.bend + c3.bend) - c4.bend
    nbz = 2.0 * (c1.bz + c2.bz + c3.bz) - c4.bz
    return Circ(nb, nbz)
end

# Fill the curvilinear triangle bounded by c1,c2,c3 (c4 is the circle on the far
# side), stopping once curvature exceeds maxbend (i.e. the circle is too small).
function recurse!(circles, c1, c2, c3, c4, maxbend)
    c5 = partner(c1, c2, c3, c4)
    if c5.bend > maxbend
        return
    end
    push!(circles, c5)
    recurse!(circles, c1, c2, c5, c3, maxbend)
    recurse!(circles, c1, c3, c5, c2, maxbend)
    recurse!(circles, c2, c3, c5, c1, maxbend)
    return
end

# Root quadruple (−1/3, 2/3, 2/3, 1): a bounding circle of radius 3 holding two
# radius-3/2 circles and two radius-1 circles (everything scaled up by 3).
function gasket(maxbend)
    c0 = Circ(-1.0 / 3.0, Complex(0.0, 0.0))  # outer bounding circle, radius 3
    c1 = Circ(2.0 / 3.0, Complex(-1.0, 0.0))  # radius 3/2, centered at (-3/2, 0)
    c2 = Circ(2.0 / 3.0, Complex(1.0, 0.0))   # radius 3/2, centered at ( 3/2, 0)
    c3 = Circ(1.0, Complex(0.0, 2.0))         # radius 1, centered at (0, 2)
    circles = Circ[c0, c1, c2, c3]
    recurse!(circles, c0, c1, c2, c3, maxbend)
    recurse!(circles, c0, c1, c3, c2, maxbend)
    recurse!(circles, c0, c2, c3, c1, maxbend)
    recurse!(circles, c1, c2, c3, c0, maxbend)
    return circles
end

circles = gasket(120.0)

# keepAspectRatio=true keeps the x and y scales equal so circles look round.
b = board(; xlim=(-3.15, 3.15), ylim=(-3.15, 3.15), axis=false, grid=false, keepAspectRatio=true)

for c in circles
    z = ccenter(c)
    r = cradius(c)
    # The outer (negative-bend) circle gets a heavier, darker outline.
    if c.bend < 0
        circ = circle((real(z), imag(z)), r; strokeColor="#0b2545", strokeWidth=2.0,
                      fillColor="none")
    else
        circ = circle((real(z), imag(z)), r; strokeColor="#1d4e89", strokeWidth=1.0,
                      fillColor="#a9c6e8", fillOpacity=0.35)
    end
    push!(b, circ)
    # Label the larger circles with their (integer) curvature, as in the article.
    if c.bend > 0 && c.bend <= 15
        lbl = text(real(z), imag(z), string(round(Int, c.bend)); anchorX="middle",
                   anchorY="middle", fontSize=13, strokeColor="#0b2545")
        push!(b, lbl)
    end
end

html(b)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "jsxgraph_lissajous_3d",
    name: "JSXGraph Lissajous 3D",
    category: "Visualization",
    description: "Render a 3D Lissajous curve with JSXGraph.jl view3d and curve3d",
    difficulty: "Intermediate",
    tags: ["jsxgraph","3d","lissajous","curve3d","visualization","interactive"],
    folder: "intermediate",
    code: `# 3D Lissajous curve rendered with JSXGraph.jl.
# Raw JavaScript coordinate expressions are carried as JSFunction values in the
# application/vnd.jsxgraph+json artifact, then evaluated by the frontend renderer.
using JSXGraph

b = board(; xlim=(-5, 5), ylim=(-5, 5), axis=false, grid=false,
          showNavigation=true, showCopyright=false)

v = view3d([-4.0, -3.0], [8.0, 8.0],
           Any[Any[-2.0, 2.0], Any[-2.0, 2.0], Any[-2.0, 2.0]];
           xPlaneRear=true, yPlaneRear=true, zPlaneRear=true)

curve = curve3d(
    "1.8*Math.sin(3*t + Math.PI/2)",
    "1.8*Math.sin(4*t)",
    "1.8*Math.sin(5*t)",
    [0.0, 2*pi];
    strokeColor="#00d1b2",
    strokeWidth=3
)

push!(v, curve)
push!(b, v)
html(b)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "jsxgraph_torus",
    name: "JSXGraph Torus",
    category: "Visualization",
    description: "Render a torus surface with JSXGraph.jl view3d and parametricsurface3d",
    difficulty: "Intermediate",
    tags: ["jsxgraph","3d","torus","parametricsurface3d","surface","visualization","interactive"],
    folder: "intermediate",
    code: `# Torus surface rendered with JSXGraph.jl, following the parametricsurface3d
# example from the JSXGraph documentation.
# The coordinate maps FX(u,v), FY(u,v), FZ(u,v) are raw JavaScript expressions
# in u and v, carried as two-argument JSFunction values in the
# application/vnd.jsxgraph+json artifact and evaluated by the frontend renderer.
using JSXGraph

b = board(; xlim=(-5, 5), ylim=(-5, 5), axis=false, grid=false,
          showNavigation=true, showCopyright=false)

v = view3d([-4.0, -3.0], [8.0, 8.0],
           Any[Any[-4.0, 4.0], Any[-4.0, 4.0], Any[-2.0, 2.0]];
           xPlaneRear=true, yPlaneRear=true, zPlaneRear=true)

# Parametric torus with major radius R = 2.5 and minor (tube) radius r = 1:
#   x = (R + r*cos(v)) * cos(u)
#   y = (R + r*cos(v)) * sin(u)
#   z =  r*sin(v)
# u sweeps around the central hole, v sweeps around the tube.
torus = parametricsurface3d(
    "(2.5 + Math.cos(v)) * Math.cos(u)",
    "(2.5 + Math.cos(v)) * Math.sin(u)",
    "Math.sin(v)",
    [0.0, 2*pi],   # range of u
    [0.0, 2*pi];   # range of v
    strokeColor="#3e8ed0",
    strokeWidth=1,
    fillColor="#00d1b2",
    fillOpacity=0.85,
    stepsU=40,
    stepsV=24
)

push!(v, torus)
push!(b, v)
html(b)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "mandelbrot_set",
    name: "Mandelbrot Set",
    category: "Algorithms",
    description: "Mandelbrot set with broadcast (f.(C, Ref(x))) for vectorized computation",
    difficulty: "Advanced",
    tags: ["mandelbrot","fractal","complex","broadcast","Ref","visualization","abs2"],
    folder: "advanced",
    code: `# Mandelbrot escape time algorithm
function mandelbrot_escape(c, maxiter)
    z = 0.0 + 0.0im
    for k in 1:maxiter
        if abs2(z) > 4.0        # |z|^2 > 4
            return k
        end
        z = z^2 + c
    end
    return maxiter
end

# Compute grid using broadcast (vectorized)
# xs' creates a row vector, ys is a column vector
# Broadcasting creates a 2D complex matrix C
function mandelbrot_grid(width, height, maxiter)
    xmin = -2.0; xmax = 1.0
    ymin = -1.2; ymax = 1.2

    xs = range(xmin, xmax; length=width)
    ys = range(ymax, ymin; length=height)

    # Create 2D complex grid via broadcasting
    C = xs' .+ im .* ys

    # Apply escape function to all points at once
    # Ref(maxiter) prevents maxiter from being broadcast
    mandelbrot_escape.(C, Ref(maxiter))
end

# ASCII visualization
@time grid = mandelbrot_grid(50, 25, 50)
println("Mandelbrot Set (50x25):")
for row in 1:25
    for col in 1:50
        n = grid[row, col]
        if n == 50
            print("#")
        elseif n > 25
            print("+")
        elseif n > 10
            print(".")
        else
            print(" ")
        end
    end
    println("")
end

println(grid[12, 25])
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "coprime_pi_estimation",
    name: "Coprime π Estimation",
    category: "Mathematics",
    description: "Estimate π using the probability that two integers are coprime",
    difficulty: "Advanced",
    tags: ["gcd","math","pi","number-theory"],
    folder: "advanced",
    code: `# Estimate π using coprime probability
# P(gcd(a,b) = 1) = 6/π² → π = √(6/P)

function mygcd(a, b)
    while b != 0
        tmp = b
        b = a % b
        a = tmp
    end
    a
end

function calc_pi(N)
    cnt = 0
    for a in 1:N
        for b in 1:N
            if mygcd(a, b) == 1
                cnt += 1
            end
        end
    end
    prob = cnt / N / N
    sqrt(6.0 / prob)
end

@time println("N=500: π ≈ ", calc_pi(500))
@time println("N=1000: π ≈ ", calc_pi(1000))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "primes_package",
    name: "Primes.jl",
    category: "Algorithms",
    description: "Primality testing, prime generation, and factorization with the Primes.jl package",
    difficulty: "Advanced",
    tags: ["primes","isprime","factor","package"],
    folder: "advanced",
    code: `# Primes.jl — fast primality testing, prime generation, and factorization.
using Primes

# isprime: probabilistic-then-deterministic primality test.
println("isprime(97)  = ", isprime(97))
println("isprime(100) = ", isprime(100))

# primes(n): all primes ≤ n as a vector.
println("primes(30)   = ", primes(30))

# factor: integer factorization as a Factorization (prime => exponent).
println("factor(360)  = ", factor(360))

# nextprime / prevprime: nearest prime above / below a value.
println("nextprime(100) = ", nextprime(100))
println("prevprime(100) = ", prevprime(100))

# totient: Euler's φ, the count of integers ≤ n coprime to n.
println("totient(36)  = ", totient(36))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "symbolics_package",
    name: "Symbolics.jl",
    category: "Algorithms",
    description: "Symbolic computing: variables, algebra, substitution, simplify/expand, and differentiation with the Symbolics.jl package",
    difficulty: "Advanced",
    tags: ["symbolics","variables","derivative","simplify","package"],
    folder: "advanced",
    code: `# Symbolics.jl — symbolic computing: variables, algebra, and calculus.
using Symbolics

# @variables declares symbolic variables.
@variables x y

# Build symbolic expressions with ordinary arithmetic operators.
ex = x^2 + 2x + 1
println("ex                 = ", ex)

# substitute: replace variables with values (folds to a number when fully numeric).
println("substitute x=>3    = ", substitute(ex, Dict(x => 3)))

# expand: distribute products and integer powers into a polynomial.
println("expand (x+y)^2     = ", expand((x + y)^2))

# simplify: combine like terms and factors.
println("simplify x+x+x     = ", simplify(x + x + x))

# derivative / Differential: symbolic differentiation (chain, product, power rules).
println("d/dx(x^2+sin(x))   = ", derivative(x^2 + sin(x), x))
println("Differential cos   = ", Differential(x)(cos(x)))
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "meta_parse_eval",
    name: "Meta.parse & eval",
    category: "Macros",
    description: "Parse strings into expressions and evaluate them dynamically",
    difficulty: "Advanced",
    tags: ["meta","parse","eval","expression","metaprogramming"],
    folder: "advanced",
    code: `# Meta.parse and eval - Metaprogramming in Julia
# Parse a string into an expression and evaluate it

# Parse a simple expression
expr = Meta.parse("1+1")
println("Expression: ", expr)
println("Head: ", expr.head)
println("Args: ", expr.args)
println("Result: ", eval(expr))

println("")

# Parse a more complex expression
expr2 = Meta.parse("2 * 3 + 4")
println("Expression: ", expr2)
println("Head: ", expr2.head)
println("Args: ", expr2.args)
println("Result: ", eval(expr2))

println("")

# Parse a function call
expr3 = Meta.parse("sqrt(16)")
println("Expression: ", expr3)
println("Head: ", expr3.head)
println("Args: ", expr3.args)
println("Result: ", eval(expr3))

println("")

# Expr variables persist correctly - can be reused
println("Verifying Expr persistence:")
println("eval(expr) = ", eval(expr))    # Still returns 2
println("eval(expr2) = ", eval(expr2))  # Still returns 10
println("eval(expr3) = ", eval(expr3))  # Still returns 4.0

eval(expr)
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "user_defined_macros",
    name: "User-Defined Macros",
    category: "Macros",
    description: "Create custom macros with quote and esc for code transformation",
    difficulty: "Advanced",
    tags: ["macro","quote","esc","metaprogramming"],
    folder: "advanced",
    code: `# User-Defined Macros - Create your own macros in Julia

# Simple macro that doubles an expression
macro twice(expr)
    quote
        \$(esc(expr))
        \$(esc(expr))
    end
end

println("=== @twice macro ===")
@twice println("Hello!")

println("")

# Macro that wraps code with before/after messages
macro debug(expr)
    quote
        println("[DEBUG] Executing...")
        result = \$(esc(expr))
        println("[DEBUG] Done. Result: ", result)
        result
    end
end

println("=== @debug macro ===")
x = @debug 1 + 2 + 3
println("x = ", x)

println("")

# Macro that adds two expressions
macro add(a, b)
    quote
        \$(esc(a)) + \$(esc(b))
    end
end

println("=== @add macro ===")
result = @add 10 20
println("@add 10 20 = ", result)

println("")

# Macro that multiplies an expression by 2
macro double(expr)
    quote
        2 * \$(esc(expr))
    end
end

println("=== @double macro ===")
println("@double 21 = ", @double 21)
println("@double 3 + 4 = ", @double 3 + 4)

result
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "matrix_decompositions",
    name: "Matrix Decompositions",
    category: "Linear Algebra",
    description: "Solve a linear system, then verify eigenvalue and SVD decompositions",
    difficulty: "Advanced",
    tags: ["linear-algebra","eigen","svd","matrix","decomposition"],
    folder: "advanced",
    code: `# Matrix decompositions with LinearAlgebra
using LinearAlgebra
using Test

# Solve a linear system A x = y
A = rand(3, 3)
y = rand(3)
x = A \\ y
@test y ≈ A * x

# Eigenvalue decomposition: A v = λ v
F = eigen(A)
v1 = F.vectors[:, 1]
λ1 = F.values[1]
@test A * v1 ≈ λ1 * v1

# Singular value decomposition: B = U Σ V'
B = rand(3, 4)
G = svd(B)
U, S, V = G
@test V' ≈ G.Vt
@test B ≈ U * Diagonal(S) * V'

true
`,
    ir: null,
    webUnsupported: false
  },
  {
    id: "distributions_package",
    name: "Distributions.jl",
    category: "Mathematics",
    description: "Probability distributions: pdf/cdf/quantile, truncation, sampling, fitting, and a StatsPlots pdf curve",
    difficulty: "Advanced",
    tags: ["distributions","normal","binomial","pdf","cdf","rand","fit","truncated","statsplots","package"],
    folder: "advanced",
    code: `# Distributions.jl — probability distributions, plotting, sampling, and fitting.
using Distributions
using StatsPlots
using Random

# Deterministic output for a reproducible demo.
Random.seed!(42)

# Continuous univariate distribution: Normal(μ, σ).
n = Normal(2.0, 3.0)
println("Normal mean/std = ", mean(n), " / ", std(n))
println("pdf(Normal, 2)  = ", pdf(n, 2.0))
println("cdf(Normal, 2)  = ", cdf(n, 2.0))
println("q95(Normal)     = ", quantile(n, 0.95))

# Truncated distributions keep the same API on a bounded support.
tn = truncated(Normal(), -1.0, 1.0)
println("truncated support = [", minimum(tn), ", ", maximum(tn), "]")
println("truncated median  = ", quantile(tn, 0.5))

# Discrete distributions: classical and recently added examples.
b = Binomial(10, 0.3)
pb = PoissonBinomial([0.2, 0.5, 0.8])
sk = Skellam(4.0, 1.5)
println("Binomial pmf/cdf at 3 = ", pdf(b, 3), " / ", cdf(b, 3))
println("PoissonBinomial mean  = ", mean(pb), ", pmf(2) = ", pdf(pb, 2))
println("Skellam mean/var      = ", mean(sk), " / ", var(sk))

# Draw samples and compute empirical statistics.
samples = [rand(n) for _ in 1:1000]
empirical_mean = sum(samples) / length(samples)
empirical_var = sum((s - empirical_mean)^2 for s in samples) / (length(samples) - 1)
println("empirical mean ≈ ", empirical_mean)
println("empirical var  ≈ ", empirical_var)

# Fit a distribution to observed data.
data = [1.0, 2.0, 3.0, 4.0, 5.0]
fit_d = fit_mle(Normal, data)
println("fit_mle mean/std = ", mean(fit_d), " / ", std(fit_d))

# StatsPlots renders a pdf curve for a distribution.
plot(Normal(); title = "Standard Normal pdf")
`,
    ir: null,
    webUnsupported: false
  }
];
