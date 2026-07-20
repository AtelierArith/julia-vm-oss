#!/usr/bin/env node
// Register VM vs stack VM Wasm measurement driver (Issue #8559).
//
// Runs the Issue #8448 benchmark set through the `RegisterVmBench` wasm
// binding (subset_julia_vm_web) under Node, with the register VM gate off
// (production stack VM) and on (eligible bodies on the register VM
// prototype). Prints the same deterministic counters as the host harness
// (`register_vm_bench_8559`) plus wall times measured around each `run`
// call (compilation happens once in the constructor and is excluded).
//
// Build the nodejs package first (out-dir may be anywhere):
//   wasm-pack build subset_julia_vm_web --target nodejs --profile web-release \
//     --out-dir /tmp/sjulia-pkg-node
// Then:
//   node scripts/register_vm_wasm_bench_8559.mjs /tmp/sjulia-pkg-node [reps]

import { createRequire } from "node:module";
import { performance } from "node:perf_hooks";
import { statSync } from "node:fs";
import { join } from "node:path";

const pkgDir = process.argv[2];
const reps = Number.parseInt(process.argv[3] ?? "5", 10);
if (!pkgDir || !Number.isInteger(reps) || reps < 1) {
  console.error(
    "usage: node scripts/register_vm_wasm_bench_8559.mjs <pkg-node-dir> [reps]"
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

function wallRuns(bench, registerGate) {
  const samples = [];
  for (let i = 0; i < reps; i += 1) {
    const start = performance.now();
    bench.run(registerGate, false, 0n);
    samples.push(performance.now() - start);
  }
  return samples;
}

const wasmSize = statSync(join(pkgDir, "subset_julia_vm_web_bg.wasm")).size;
console.log(
  `# register_vm_wasm_bench_8559 (target: wasm32-unknown-unknown/node ${process.version}, reps: ${reps})`
);
console.log(`# wasm artifact: subset_julia_vm_web_bg.wasm ${wasmSize} bytes`);

let parityFailed = false;
for (const { name, src, expected } of BENCHES) {
  const bench = new wasm.RegisterVmBench(src);

  // Deterministic counters, both engines.
  const off = bench.run(false, true, 0n);
  const on = bench.run(true, true, 0n);
  if (off.output !== expected || on.output !== expected) {
    parityFailed = true;
    console.log(
      `PARITY FAIL ${name}: expected ${JSON.stringify(expected)}, ` +
        `stack ${JSON.stringify(off.output)}, register ${JSON.stringify(on.output)}`
    );
  }

  console.log(`\n## ${name}`);
  console.log(
    `counters[stack-vm  ]: dispatches=${off.stack_dispatches} ` +
      `executable_blocks=${off.executable_block_runs} ` +
      `operand_stack_high_water=${off.operand_stack_high_water} ` +
      `frames_high_water=${off.frames_high_water}`
  );
  console.log(
    `counters[register  ]: register_calls=${on.register_calls} ` +
      `register_fallbacks=${on.register_fallbacks} ` +
      `register_dispatches=${on.register_dispatches}`
  );
  console.log(
    `counters[reg-resid ]: stack_dispatches=${on.stack_dispatches} ` +
      `executable_blocks=${on.executable_block_runs} ` +
      `operand_stack_high_water=${on.operand_stack_high_water} ` +
      `frames_high_water=${on.frames_high_water}`
  );

  // Wall times (uninstrumented).
  const offMs = wallRuns(bench, false);
  const onMs = wallRuns(bench, true);
  const fmt = (xs) => xs.map((x) => x.toFixed(3)).join(", ");
  console.log(
    `wall_ms: stack median=${median(offMs).toFixed(3)} min=${Math.min(...offMs).toFixed(3)} | ` +
      `register median=${median(onMs).toFixed(3)} min=${Math.min(...onMs).toFixed(3)} ` +
      `(samples: stack [${fmt(offMs)}] register [${fmt(onMs)}])`
  );

  bench.free();
}

if (parityFailed) {
  console.log("\nPARITY FAILURES DETECTED");
  process.exit(1);
}
console.log(
  "\nall benchmarks matched the upstream-Julia-pinned output on both engines"
);
