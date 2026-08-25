import assert from "node:assert/strict";
import test from "node:test";

import { createSokarisHostImports, readDescriptor } from "../../scripts/sokaris_host_imports.mjs";

function fixture() {
  const memory = new WebAssembly.Memory({ initial: 1 });
  let next = 1024;
  const allocate = (size, align) => {
    const mask = align - 1;
    next = (next + mask) & ~mask;
    const pointer = next;
    next += Number(size);
    return pointer;
  };
  return { memory, allocate };
}

test("load writes an owned ABI-v2 RGBA descriptor", () => {
  const { memory, allocate } = fixture();
  const path = new TextEncoder().encode("image.png");
  new Uint8Array(memory.buffer, 64, path.length).set(path);
  const imports = createSokarisHostImports({
    memory,
    allocate,
    loadImage: (value) => {
      assert.equal(value, "image.png");
      return { width: 2, height: 1, pixels: [1, 2, 3, 4, 5, 6, 7, 8] };
    },
    saveImage: () => {},
    renderText: () => ({ width: 1, height: 1, pixels: [0, 0, 0, 0] }),
  });
  assert.equal(imports.sjulia_host.load(64, path.length, 0, 128), 0);
  const descriptor = readDescriptor(memory, 128);
  assert.equal(descriptor.rank, 3);
  assert.deepEqual(descriptor.dimensions, [4n, 2n, 1n]);
  assert.deepEqual(descriptor.strides, [1n, 4n, 8n]);
  assert.deepEqual(Array.from(new Uint8Array(memory.buffer, descriptor.dataPointer, 8)), [1, 2, 3, 4, 5, 6, 7, 8]);
});

test("save validates and forwards the descriptor", () => {
  const { memory, allocate } = fixture();
  const path = new TextEncoder().encode("out.png");
  new Uint8Array(memory.buffer, 64, path.length).set(path);
  const descriptor = allocate(72n, 8);
  const pixels = [9, 8, 7, 6];
  const imports = createSokarisHostImports({
    memory,
    allocate,
    loadImage: () => ({ width: 1, height: 1, pixels }),
    saveImage: (value, image) => {
      assert.equal(value, "out.png");
      assert.equal(image.elementCount, 4n);
    },
    renderText: () => ({ width: 1, height: 1, pixels }),
  });
  assert.equal(imports.sjulia_host.load(64, path.length, 0, descriptor), 0);
  assert.equal(imports.sjulia_host.save(64, path.length, descriptor), 0);
});

test("typed load and save use String views, Int64 pointers, and Int64 statuses", () => {
  const { memory, allocate } = fixture();
  const path = new TextEncoder().encode("inputs/image.png");
  const pathData = allocate(BigInt(path.length), 1);
  new Uint8Array(memory.buffer, pathData, path.length).set(path);
  const pathView = allocate(8n, 4);
  const view = new DataView(memory.buffer);
  view.setUint32(pathView, pathData, true);
  view.setUint32(pathView + 4, path.length, true);
  let saved;
  const imports = createSokarisHostImports({
    memory,
    allocate,
    loadImage: (value) => {
      assert.equal(value, "inputs/image.png");
      return { width: 1, height: 1, pixels: [1, 2, 3, 4] };
    },
    saveImage: (value, image) => {
      saved = { value, image };
    },
    renderText: () => ({ width: 1, height: 1, pixels: [0, 0, 0, 0] }),
  });

  const descriptor = allocate(88n, 8);
  assert.equal(imports.sjulia_host.load(pathView, 0n, BigInt(descriptor)), 0n);
  assert.equal(readDescriptor(memory, descriptor).elementCount, 4n);
  assert.equal(imports.sjulia_host.save(pathView, BigInt(descriptor)), 0n);
  assert.equal(saved.value, "inputs/image.png");
  assert.equal(saved.image.pointer, descriptor);
});

test("text overlay maps renderer output and failures to stable statuses", () => {
  const { memory, allocate } = fixture();
  const source = new TextEncoder().encode("hello");
  new Uint8Array(memory.buffer, 64, source.length).set(source);
  const imports = createSokarisHostImports({
    memory,
    allocate,
    loadImage: () => ({ width: 1, height: 1, pixels: [0, 0, 0, 0] }),
    saveImage: () => {},
    renderText: ({ source: value, width, height }) => {
      assert.deepEqual([value, width, height], ["hello", 1, 1]);
      return { width, height, pixels: [10, 20, 30, 40] };
    },
  });
  assert.equal(imports.sjulia_host.text_overlay(64, source.length, 1n, 1n, 128), 0);
  assert.equal(readDescriptor(memory, 128).elementCount, 4n);

  const failing = createSokarisHostImports({
    memory,
    allocate,
    loadImage: () => { throw Object.assign(new Error("missing"), { code: "ENOENT" }); },
    saveImage: () => {},
    renderText: () => { throw new Error("renderer"); },
  });
  assert.equal(failing.sjulia_host.load(64, source.length, 0, 128), 2);
  assert.equal(failing.sjulia_host.text_overlay(64, source.length, 1n, 1n, 128), 5);
});
