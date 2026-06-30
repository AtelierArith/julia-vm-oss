using QuadGK

function quadgk_domains_segbuf_contract_8288()
    v1, e1 = quadgk(x -> x, 0.0, 0.5, 1.0, rtol=1e-3)
    ok_vararg_domains = abs(v1 - 0.5) < 1e-12 && e1 < 1e-10

    v2, e2 = quadgk(x -> x^2, [0.0, 0.5, 1.0], rtol=1e-3)
    ok_vector_domains = abs(v2 - 1 / 3) < 1e-12 && e2 < 1e-10

    v3, e3 = quadgk(x -> x^2, [(0.0, 0.5), (0.5, 1.0)], rtol=1e-3)
    ok_tuple_domains = abs(v3 - 1 / 3) < 1e-12 && e3 < 1e-10

    v4, e4, segs = quadgk_segbuf(x -> x^2, 0.0, 1.0)
    ok_segbuf_result = abs(v4 - 1 / 3) < 1e-12 && e4 < 1e-10 && length(segs) == 1

    v5, e5 = quadgk(x -> x, 0.0, 1.0, eval_segbuf=segs, maxevals=0)
    ok_eval_segbuf = abs(v5 - 0.5) < 1e-12 && e5 == 0.0

    _, _, segbuf = quadgk_segbuf(sin, 0.0, 1.0, maxevals=0)
    v6, e6 = quadgk(x -> x^2, 0.0, 1.0, segbuf=segbuf, maxevals=10)
    ok_alloc_segbuf = abs(v6 - 1 / 3) < 1e-12 && e6 < 1e-10 && length(segbuf) == 1

    ok_vararg_domains &&
        ok_vector_domains &&
        ok_tuple_domains &&
        ok_segbuf_result &&
        ok_eval_segbuf &&
        ok_alloc_segbuf
end

quadgk_domains_segbuf_contract_8288()
