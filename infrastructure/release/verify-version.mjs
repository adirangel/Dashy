import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const TAG_PATTERN = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const VERSION_PATTERN = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const VERSION_SOURCES = Object.freeze({
  tauri: "backend/tauri.conf.json",
  cargo: "backend/Cargo.toml",
  frontend: "frontend/package.json",
});
const VERSION_KEYS = Object.freeze(Object.keys(VERSION_SOURCES));

export function validateReleaseVersions(tag, versions) {
  const match = typeof tag === "string" ? TAG_PATTERN.exec(tag) : null;
  if (!match) {
    throw new Error("Release tag must use exact stable vMAJOR.MINOR.PATCH format.");
  }

  const source = versions && typeof versions === "object" && !Array.isArray(versions)
    ? versions
    : {};
  const providedKeys = Object.keys(source);
  const missing = VERSION_KEYS.filter(
    (key) => !Object.prototype.hasOwnProperty.call(source, key),
  );
  const unknown = providedKeys.filter((key) => !VERSION_KEYS.includes(key));
  if (missing.length || unknown.length) {
    const problems = [];
    if (missing.length) problems.push(`missing fields: ${missing.join(", ")}`);
    if (unknown.length) problems.push(`unknown fields: ${unknown.join(", ")}`);
    throw new Error(`Release versions have ${problems.join("; ")}.`);
  }

  const expected = match.slice(1).join(".");
  const mismatches = VERSION_KEYS
    .filter((key) => source[key] !== expected)
    .map((key) => `${VERSION_SOURCES[key]}=${String(source[key])}`);
  if (mismatches.length) {
    throw new Error(`Tag ${tag} does not match ${mismatches.join(", ")}.`);
  }
}

function readManifest(root, relativePath) {
  try {
    return fs.readFileSync(path.join(root, ...relativePath.split("/")), "utf8");
  } catch {
    throw new Error(`${relativePath} could not be read.`);
  }
}

function readJsonManifest(root, relativePath) {
  const source = readManifest(root, relativePath);
  try {
    return JSON.parse(source);
  } catch {
    throw new Error(`${relativePath} is not valid JSON.`);
  }
}

function jsonManifestVersion(root, relativePath) {
  const manifest = readJsonManifest(root, relativePath);
  const version = manifest && typeof manifest === "object" && !Array.isArray(manifest)
    ? manifest.version
    : undefined;
  if (typeof version !== "string" || !VERSION_PATTERN.test(version)) {
    throw new Error(`${relativePath} has no valid version.`);
  }
  return version;
}

function cargoPackageVersion(source) {
  const packageHeader = /^\s*\[package\]\s*(?:#.*)?$/m.exec(source);
  if (!packageHeader) {
    throw new Error("backend/Cargo.toml has no [package] section.");
  }

  const bodyStart = packageHeader.index + packageHeader[0].length;
  const remainder = source.slice(bodyStart);
  const nextSection = /^\s*\[[^\]\r\n]+\].*$/m.exec(remainder);
  const packageSection = remainder.slice(0, nextSection?.index);
  const versionMatch = /^\s*version\s*=\s*"([^"\r\n]+)"\s*(?:#.*)?$/m.exec(packageSection);
  if (!versionMatch || !VERSION_PATTERN.test(versionMatch[1])) {
    throw new Error("backend/Cargo.toml has no valid stable package version.");
  }
  return versionMatch[1];
}

export function readReleaseVersions(root = process.cwd()) {
  return {
    tauri: jsonManifestVersion(root, VERSION_SOURCES.tauri),
    cargo: cargoPackageVersion(readManifest(root, VERSION_SOURCES.cargo)),
    frontend: jsonManifestVersion(root, VERSION_SOURCES.frontend),
  };
}

const entry = process.argv[1]
  && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;

if (entry) {
  try {
    const tag = process.argv[2] ?? "";
    validateReleaseVersions(tag, readReleaseVersions());
    process.stdout.write(`Release version ${tag} is consistent.\n`);
  } catch (error) {
    const message = error instanceof Error
      ? error.message
      : "Release version validation failed.";
    process.stderr.write(`${message}\n`);
    process.exitCode = 1;
  }
}
