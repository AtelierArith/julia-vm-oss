using JSXGraph

# Apollonian gasket via the linear Descartes "swap" b₄′ = 2(b₁+b₂+b₃) − b₄,
# tracking bend·center as a complex number. Matches the (−1,2,2,3) root quadruple.
struct Circ
    bend::Float64
    bz::Complex{Float64}
end

ccenter(c::Circ) = c.bz / c.bend
cradius(c::Circ) = 1.0 / abs(c.bend)

function partner(c1::Circ, c2::Circ, c3::Circ, c4::Circ)
    Circ(2.0 * (c1.bend + c2.bend + c3.bend) - c4.bend,
         2.0 * (c1.bz + c2.bz + c3.bz) - c4.bz)
end

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

c0 = Circ(-1.0, Complex(0.0, 0.0))
c1 = Circ(2.0, Complex(-1.0, 0.0))
c2 = Circ(2.0, Complex(1.0, 0.0))
c3 = Circ(3.0, Complex(0.0, 2.0))
circles = Circ[c0, c1, c2, c3]
recurse!(circles, c0, c1, c2, c3, 40.0)
recurse!(circles, c0, c1, c3, c2, 40.0)
recurse!(circles, c0, c2, c3, c1, 40.0)
recurse!(circles, c1, c2, c3, c0, 40.0)

bends = sort([round(Int, c.bend) for c in circles])

# Build a board of circles with coordinate-tuple centers (no point elements).
b = board(; xlim=(-1.05, 1.05), ylim=(-1.05, 1.05), axis=false, grid=false)
for c in circles
    z = ccenter(c)
    push!(b, circle((real(z), imag(z)), cradius(c)))
end

# The central circle (partner of the outer circle) has curvature
# 2(2+2+3) − (−1) = 15; the two radius-⅓ circles have curvature 3.
length(circles) == 57 &&
    bends[1] == -1 &&
    count(==(3), bends) == 2 &&
    15 in bends &&
    length(b.elements) == 57 &&
    b.elements[1].type_name == :circle
