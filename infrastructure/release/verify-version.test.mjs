import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

import {
  readReleaseVersions,
  validateReleaseVersions,
} from "./verify-version.mjs";

const matching = { tauri: "0.1.0", cargo: "0.1.0", frontend: "0.1.0" };
const scriptPath = path.resolve("infrastructure/release/verify-version.mjs");

async function createManifests({
  tauri = JSON.stringify({ version: "0.1.0" }),
  cargo = '[package]\nname = "dashy"\nversion = "0.1.0"\n',
  frontend = JSON.stringify({ version: "0.1.0" }),
} = {}) {
  const root = await mkdtemp(path.join(tmpdir(), "dashy-release-version-"));
  await mkdir(path.join(root, "backend"), { recursive: true });
  await mkdir(path.join(root, "frontend"), { recursive: true });
  await writeFile(path.join(root, "backend", "tauri.conf.json"), tauri);
  await writeFile(path.join(root, "backend", "Cargo.toml"), cargo);
  await writeFile(path.join(root, "frontend", "package.json"), frontend);
  return root;
}

test("accepts one exact semantic version across tag and manifests", () => {
  assert.doesNotThrow(() => validateReleaseVersions("v0.1.0", matching));
});

test("rejects malformed and prerelease tags", () => {
  for (const tag of [
    "0.1.0",
    "release-0.1.0",
    "v0.1",
    "v0.1.0-beta.1",
    "v01.1.0",
    "v1.01.0",
    "v1.0.01",
    "v1.0.0 ",
  ]) {
    assert.throws(
      () => validateReleaseVersions(tag, matching),
      /vMAJOR\.MINOR\.PATCH/,
      tag,
    );
  }
});

test("reports every manifest that disagrees with the tag", () => {
  assert.throws(
    () =>
      validateReleaseVersions("v0.2.0", {
        tauri: "0.2.0",
        cargo: "0.1.0",
        frontend: "0.3.0",
      }),
    /backend\/Cargo\.toml=0\.1\.0.*frontend\/package\.json=0\.3\.0/,
  );
});

test("rejects missing and unknown version fields", () => {
  assert.throws(
    () => validateReleaseVersions("v0.1.0", { tauri: "0.1.0", cargo: "0.1.0" }),
    /missing.*frontend/i,
  );
  assert.throws(
    () => validateReleaseVersions("v0.1.0", { ...matching, desktop: "0.1.0" }),
    /unknown.*desktop/i,
  );
});

test("reads only the Cargo package version, ignoring decoy version keys", async () => {
  const root = await createManifests({
    cargo: [
      '[workspace.package]',
      'version = "9.9.9"',
      '',
      '[package]',
      'name = "dashy"',
      'version = "0.1.0" # release version',
      '',
      '[dependencies.example]',
      'version = "8.8.8"',
      '',
    ].join("\n"),
  });

  assert.deepEqual(readReleaseVersions(root), matching);
});

test("rejects Cargo manifests without an exact package version", async () => {
  const noPackage = await createManifests({
    cargo: '[workspace.package]\nversion = "0.1.0"\n',
  });
  assert.throws(() => readReleaseVersions(noPackage), /Cargo\.toml.*\[package\]/);

  const noVersion = await createManifests({
    cargo: '[package]\nname = "dashy"\n[dependencies]\nversion = "0.1.0"\n',
  });
  assert.throws(() => readReleaseVersions(noVersion), /Cargo\.toml.*package version/);
});

test("reports malformed JSON and missing manifest files safely", async () => {
  const malformed = await createManifests({ tauri: "{" });
  assert.throws(() => readReleaseVersions(malformed), /tauri\.conf\.json.*valid JSON/);

  const root = await mkdtemp(path.join(tmpdir(), "dashy-release-version-missing-"));
  assert.throws(
    () => readReleaseVersions(root),
    (error) =>
      error instanceof Error &&
      /backend\/tauri\.conf\.json.*could not be read/.test(error.message) &&
      !error.message.includes(root),
  );
});

test("rejects manifests with missing or invalid version values", async () => {
  const missing = await createManifests({ tauri: JSON.stringify({ productName: "Dashy" }) });
  assert.throws(() => readReleaseVersions(missing), /tauri\.conf\.json.*valid version/);

  const numeric = await createManifests({ frontend: JSON.stringify({ version: 1 }) });
  assert.throws(() => readReleaseVersions(numeric), /frontend\/package\.json.*valid version/);
});

test("CLI succeeds for matching manifests and emits a concise confirmation", async () => {
  const root = await createManifests();
  const result = spawnSync(process.execPath, [scriptPath, "v0.1.0"], {
    cwd: root,
    encoding: "utf8",
  });

  assert.equal(result.status, 0);
  assert.equal(result.stdout, "Release version v0.1.0 is consistent.\n");
  assert.equal(result.stderr, "");
});

test("CLI fails safely and reports every mismatch", async () => {
  const root = await createManifests();
  const result = spawnSync(process.execPath, [scriptPath, "v9.9.9"], {
    cwd: root,
    encoding: "utf8",
  });

  assert.equal(result.status, 1);
  assert.equal(result.stdout, "");
  assert.match(result.stderr, /backend\/tauri\.conf\.json=0\.1\.0/);
  assert.match(result.stderr, /backend\/Cargo\.toml=0\.1\.0/);
  assert.match(result.stderr, /frontend\/package\.json=0\.1\.0/);
  assert.doesNotMatch(result.stderr, new RegExp(root.replaceAll("\\", "\\\\")));
});
