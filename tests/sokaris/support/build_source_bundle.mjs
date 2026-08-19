import { createHash } from "node:crypto";
import { readFile, realpath } from "node:fs/promises";
import { isAbsolute, relative } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = new URL("../../../", import.meta.url);
const defaultSokarisRoot = new URL("../sokaris/", repoRoot);
const defaultFixtures = new URL("../fixtures/harness-mechanics.json", import.meta.url);

export class SourceBundleError extends Error {
  constructor(code, message, details = undefined) {
    super(message);
    this.name = "SourceBundleError";
    this.code = code;
    this.details = details;
  }
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

export async function loadHarnessFixtures(path = defaultFixtures) {
  let parsed;
  try {
    parsed = JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    throw new SourceBundleError("fixture_manifest_invalid", `cannot read harness fixtures: ${error.message}`);
  }
  if (parsed.schemaVersion !== 1 || parsed.cases === null || typeof parsed.cases !== "object") {
    throw new SourceBundleError("fixture_manifest_invalid", "harness fixtures require schemaVersion 1 and a cases object");
  }
  return parsed.cases;
}

export async function buildSourceBundle({ row, fixture, sokarisRoot = defaultSokarisRoot }) {
  if (fixture === undefined) {
    throw new SourceBundleError(
      "fixture_not_implemented",
      `${row.caseId} remains planned; no executable compiler fixture exists yet`,
    );
  }
  const rootPath = fileURLToPath(sokarisRoot);
  const candidatePath = fileURLToPath(new URL(row.source.file, sokarisRoot));
  const candidateRelativePath = relative(rootPath, candidatePath);
  if (candidateRelativePath === "" || isAbsolute(candidateRelativePath) || candidateRelativePath.startsWith("..")) {
    throw new SourceBundleError("source_path_escape", `${row.caseId} source file escapes the Sokaris root`);
  }
  const canonicalRootPath = await realpath(rootPath);
  const sourcePath = await realpath(candidatePath);
  const sourceRelativePath = relative(canonicalRootPath, sourcePath);
  if (isAbsolute(sourceRelativePath) || sourceRelativePath.startsWith("..")) {
    throw new SourceBundleError("source_path_escape", `${row.caseId} source file escapes the canonical Sokaris root`);
  }
  const source = await readFile(sourcePath, "utf8");
  const lines = source.split(/\r?\n/u);
  const sourceSpan = lines.slice(row.source.startLine - 1, row.source.endLine).join("\n");
  const actualHash = sha256(sourceSpan);
  if (actualHash !== row.source.sha256) {
    throw new SourceBundleError("source_span_hash_mismatch", `${row.caseId} source span hash is stale`, {
      expected: row.source.sha256,
      actual: actualHash,
    });
  }
  if (typeof fixture.compiler?.source !== "string" || fixture.compiler.source.trim() === "") {
    throw new SourceBundleError("fixture_not_implemented", `${row.caseId} has no compiler source`);
  }
  const bundle = `${sourceSpan}\n${fixture.compiler.source.trimEnd()}\n`;
  if (!bundle.startsWith(sourceSpan)) {
    throw new SourceBundleError("source_span_copy_failed", `${row.caseId} source span was not copied verbatim`);
  }
  return { bundle, sourceSpan, sourceSpanSha256: actualHash };
}
