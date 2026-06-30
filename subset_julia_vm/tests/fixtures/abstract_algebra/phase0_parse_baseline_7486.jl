doc_src = "@doc raw\"\"\"\nAbstractAlgebra Phase 0 seed docstring.\n\"\"\""
module_src = "module AbstractAlgebraPhase0Seed\nconst import_exclude = [:import_exclude, :QQ, :ZZ, :Set, :Module, :Group]\nfunction generators end\nabstract type Ring end\nabstract type RingElem end\nabstract type PolyRingElem{T} <: RingElem end\nabstract type NCPolyRingElem{T} <: RingElem end\nconst PolynomialElem{T} = Union{PolyRingElem{T}, NCPolyRingElem{T}}\nend\n"

doc_ex = Meta.parse(doc_src)
module_ex = Meta.parse(module_src)
ok = doc_ex.head == :macrocall &&
     length(doc_ex.args) >= 2 &&
     length(module_ex.args) >= 2

println((doc_ex.head, module_ex.head, ok))
ok
