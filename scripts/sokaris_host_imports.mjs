const ABI_VERSION = 2;
const HEADER_BYTES = 40;
const AXIS_BYTES = 16;
const TAG_U8 = 1;
const FLAG_MODULE_OWNED = 1;
const FLAG_MUTABLE = 2;
const FLAG_CONTIGUOUS = 4;

export class HostImportError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "HostImportError";
    this.code = code;
  }
}

function memoryView(memory) {
  return new DataView(memory.buffer);
}

function checkedRange(memory, pointer, bytes, context) {
  if (!Number.isSafeInteger(pointer) || pointer < 0 || !Number.isSafeInteger(bytes) || bytes < 0) {
    throw new HostImportError("invalid_pointer", `${context} has an invalid range`);
  }
  const end = pointer + bytes;
  if (!Number.isSafeInteger(end) || end > memory.buffer.byteLength) {
    throw new HostImportError("out_of_bounds", `${context} exceeds linear memory`);
  }
}

function readUtf8(memory, pointer, length) {
  checkedRange(memory, pointer, length, "UTF-8 input");
  return new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(memory.buffer, pointer, length));
}

export function readDescriptor(memory, pointer) {
  checkedRange(memory, pointer, HEADER_BYTES, "descriptor");
  if (pointer === 0 || pointer % 8 !== 0) throw new HostImportError("invalid_descriptor", "descriptor must be nonzero and 8-byte aligned");
  const view = memoryView(memory);
  if (view.getUint32(pointer, true) !== ABI_VERSION) throw new HostImportError("invalid_descriptor", "descriptor ABI version mismatch");
  const flags = view.getUint32(pointer + 4, true);
  const elementTag = view.getUint32(pointer + 8, true);
  const elementSize = view.getUint32(pointer + 12, true);
  const layoutId = view.getUint32(pointer + 16, true);
  const rank = view.getUint32(pointer + 20, true);
  const dataPointer = view.getUint32(pointer + 24, true);
  const elementCount = view.getBigUint64(pointer + 32, true);
  if (rank > 8) throw new HostImportError("invalid_descriptor", "descriptor rank exceeds 8");
  checkedRange(memory, pointer, HEADER_BYTES + rank * AXIS_BYTES, "descriptor metadata");
  const dimensions = [];
  const strides = [];
  for (let axis = 0; axis < rank; axis += 1) {
    dimensions.push(view.getBigUint64(pointer + HEADER_BYTES + axis * AXIS_BYTES, true));
    strides.push(view.getBigInt64(pointer + HEADER_BYTES + axis * AXIS_BYTES + 8, true));
  }
  checkedRange(memory, dataPointer, Number(elementCount) * elementSize, "descriptor data");
  return { pointer, flags, elementTag, elementSize, layoutId, rank, dataPointer, elementCount, dimensions, strides };
}

function writeImageDescriptor(memory, allocate, outputPointer, pixels, width, height) {
  const bytes = Uint8Array.from(pixels);
  if (bytes.length !== width * height * 4) throw new HostImportError("invalid_image", "RGBA byte length does not match dimensions");
  const dataPointer = allocate(BigInt(bytes.length), 1);
  if (dataPointer === 0) return 6;
  new Uint8Array(memory.buffer, dataPointer, bytes.length).set(bytes);
  checkedRange(memory, outputPointer, HEADER_BYTES + 2 * AXIS_BYTES, "output descriptor");
  const view = memoryView(memory);
  view.setUint32(outputPointer, ABI_VERSION, true);
  view.setUint32(outputPointer + 4, FLAG_MODULE_OWNED | FLAG_MUTABLE | FLAG_CONTIGUOUS, true);
  view.setUint32(outputPointer + 8, TAG_U8, true);
  view.setUint32(outputPointer + 12, 1, true);
  view.setUint32(outputPointer + 16, 0, true);
  view.setUint32(outputPointer + 20, 2, true);
  view.setUint32(outputPointer + 24, dataPointer, true);
  view.setUint32(outputPointer + 28, 0, true);
  view.setBigUint64(outputPointer + 32, BigInt(bytes.length), true);
  view.setBigUint64(outputPointer + 40, BigInt(height), true);
  view.setBigInt64(outputPointer + 48, 1n, true);
  view.setBigUint64(outputPointer + 56, BigInt(width), true);
  view.setBigInt64(outputPointer + 64, BigInt(height), true);
  return 0;
}

export function createSokarisHostImports({ memory, allocate, loadImage, saveImage, renderText }) {
  const status = (action) => {
    try {
      action();
      return 0;
    } catch (error) {
      if (error instanceof HostImportError) return 1;
      return 5;
    }
  };
  return {
    sjulia_host: {
      load(pathPointer, pathLength, _layoutId, outputPointer) {
        try {
          const image = loadImage(readUtf8(memory, pathPointer, pathLength));
          return writeImageDescriptor(memory, allocate, outputPointer, image.pixels, image.width, image.height);
        } catch (error) {
          if (error?.code === "ENOENT") return 2;
          if (error?.code === "EACCES") return 3;
          if (error instanceof TypeError) return 4;
          return error instanceof HostImportError ? 1 : 5;
        }
      },
      save(pathPointer, pathLength, imagePointer) {
        return status(() => saveImage(readUtf8(memory, pathPointer, pathLength), readDescriptor(memory, imagePointer)));
      },
      text_overlay(pathPointer, pathLength, height, width, outputPointer) {
        try {
          const image = renderText({ source: readUtf8(memory, pathPointer, pathLength), height: Number(height), width: Number(width) });
          return writeImageDescriptor(memory, allocate, outputPointer, image.pixels, image.width, image.height);
        } catch (error) {
          return error instanceof HostImportError ? 1 : 5;
        }
      },
    },
  };
}
