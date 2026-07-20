#!/usr/bin/env node
// Handler-table vs match dispatch Wasm measurement driver (Issue #8562).
//
// Runs the Issue #8562 benchmark set through the `HandlerTableBench` wasm
// binding (subset_julia_vm_web, built with `--features vm-handler-table`)
// under Node, with the handler-table gate off (production `match` dispatch)
// and on (function-pointer handler table). Prints the same deterministic
// counters as the host harness (`handler_table_bench_8562`) plus wall times
// measured around each `run` call (compilation happens once in the
// constructor and is excluded).
//
// Build the nodejs package first (out-dir may be anywhere):
//   wasm-pack build subset_julia_vm_web --target nodejs --profile web-release \
//     --out-dir /tmp/sjulia-pkg-node-8562 -- --features vm-handler-table
// Then:
//   node scripts/handler_table_wasm_bench_8562.mjs /tmp/sjulia-pkg-node-8562 [reps]

import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
import { statSync } from "node:fs";
import { join } from "node:path";

const pkgDir = process.argv[2];
const reps = Number.parseInt(process.argv[3] ?? "7", 10);
if (!pkgDir || !Number.isInteger(reps) || reps < 1) {
  console.error(
    "usage: node scripts/handler_table_wasm_bench_8562.mjs <pkg-node-dir> [reps]"
  );
  process.exit(2);
}

const require = createRequire(import.meta.url);
const wasm = require(join(pkgDir, "subset_julia_vm_web.js"));

const BENCHES = [
  {
    name: "fib(25)",
    expected: "75025\n",
    src: `
function fib(n::Int64)
    if n <= 1
        return n
    end
    return fib(n - 1) + fib(n - 2)
end

println(fib(25))
`,
  },
  {
    name: "calc_pi(1_000_000)",
    expected: "3.1415916535897743\n",
    src: `
function calc_pi(n::Int64)
    acc = 0.0
    sign = 1.0
    k = 0
    while k < n
        acc = acc + sign / (2.0 * k + 1.0)
        sign = -sign
        k = k + 1
    end
    return 4.0 * acc
end

println(calc_pi(1000000))
`,
  },
  {
    name: "calc_pi_call(1_000_000)",
    expected: "3.1415916535897743\n",
    src: `
function pi_term(k::Int64)
    sign = 1.0 - 2.0 * (k % 2)
    return sign / (2.0 * k + 1.0)
end

function calc_pi_call(n::Int64)
    acc = 0.0
    k = 0
    while k < n
        acc = acc + pi_term(k)
        k = k + 1
    end
    return 4.0 * acc
end

println(calc_pi_call(1000000))
`,
  },
  {
    name: "lorenz_accum(1_000_000)",
    expected: "-11779.830551874697\n",
    src: `
function lorenz_accum(n::Int64)
    x = 1.0
    y = 1.0
    z = 1.0
    dt = 0.001
    acc = 0.0
    k = 0
    while k < n
        dx = 10.0 * (y - x)
        dy = x * (28.0 - z) - y
        dz = x * y - 2.6666666666666665 * z
        x = x + dt * dx
        y = y + dt * dy
        z = z + dt * dz
        acc = acc + x
        k = k + 1
    end
    return acc
end

println(lorenz_accum(1000000))
`,
  },
];

function median(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[mid - 1] + sorted[mid]) / 2
    : sorted[mid];
}

function wallRuns(bench, handlerTable) {
  const samples = [];
  for (let i = 0; i < reps; i += 1) {
    const start = performance.now();
    bench.run(handlerTable, false, 0n);
    samples.push(performance.now() - start);
  }
  return samples;
}

const wasmSize = statSync(join(pkgDir, "subset_julia_vm_web_bg.wasm")).size;
console.log(
  `# handler_table_wasm_bench_8562 (target: wasm32-unknown-unknown/node ${process.version}, reps: ${reps})`
);
console.log(`# wasm artifact: subset_julia_vm_web_bg.wasm ${wasmSize} bytes`);

let parityFailed = false;
for (const { name, src, expected } of BENCHES) {
  const bench = new wasm.HandlerTableBench(src);

  // Deterministic counters, both dispatch mechanisms.
  const off = bench.run(false, true, 0n);
  const on = bench.run(true, true, 0n);
  if (off.output !== expected || on.output !== expected) {
    parityFailed = true;
    console.log(
      `PARITY FAIL ${name}: expected ${JSON.stringify(expected)}, ` +
        `match ${JSON.stringify(off.output)}, table ${JSON.stringify(on.output)}`
    );
  }
  if (
    off.stack_dispatches !== on.stack_dispatches ||
    off.executable_block_runs !== on.executable_block_runs
  ) {
    parityFailed = true;
    console.log(
      `COUNTER MISMATCH ${name}: match dispatches=${off.stack_dispatches} ` +
        `blocks=${off.executable_block_runs} vs table dispatches=${on.stack_dispatches} ` +
        `blocks=${on.executable_block_runs}`
    );
  }

  console.log(`\n## ${name}`);
  console.log(
    `counters[match ]: dispatches=${off.stack_dispatches} ` +
      `executable_blocks=${off.executable_block_runs}`
  );
  const total = on.table_hits + on.table_fallbacks;
  const coverage = total === 0 ? 0 : (100 * on.table_hits) / total;
  console.log(
    `counters[table ]: table_hits=${on.table_hits} ` +
      `table_fallbacks=${on.table_fallbacks} hot_coverage=${coverage.toFixed(2)}%`
  );

  // Wall times (uninstrumented).
  const offMs = wallRuns(bench, false);
  const onMs = wallRuns(bench, true);
  const fmt = (xs) => xs.map((x) => x.toFixed(3)).join(", ");
  console.log(
    `wall_ms: match median=${median(offMs).toFixed(3)} min=${Math.min(...offMs).toFixed(3)} | ` +
      `table median=${median(onMs).toFixed(3)} min=${Math.min(...onMs).toFixed(3)} ` +
      `(samples: match [${fmt(offMs)}] table [${fmt(onMs)}])`
  );

  bench.free();
}

if (parityFailed) {
  console.log("\nPARITY FAILURES DETECTED");
  process.exit(1);
}
console.log(
  "\nall benchmarks matched the upstream-Julia-pinned output on both dispatch paths"
);
