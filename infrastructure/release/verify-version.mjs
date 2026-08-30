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

function tomlCodeLines(source) {
  let stringMode = "normal";
  const lines = [];

  for (const sourceLine of source.split(/\r?\n/)) {
    const code = Array(sourceLine.length).fill(" ");
    let index = 0;

    while (index < sourceLine.length) {
      if (stringMode === "multiline-basic") {
        if (sourceLine[index] === "\\") {
          index += Math.min(2, sourceLine.length - index);
        } else if (sourceLine[index] === '"') {
          let quoteEnd = index;
          while (sourceLine[quoteEnd] === '"') quoteEnd += 1;
          if (quoteEnd - index >= 3) stringMode = "normal";
          index = quoteEnd;
        } else {
          index += 1;
        }
        continue;
      }

      if (stringMode === "multiline-literal") {
        if (sourceLine[index] === "'") {
          let quoteEnd = index;
          while (sourceLine[quoteEnd] === "'") quoteEnd += 1;
          if (quoteEnd - index >= 3) stringMode = "normal";
          index = quoteEnd;
        } else {
          index += 1;
        }
        continue;
      }

      const character = sourceLine[index];
      if (character === "#") break;

      if (sourceLine.startsWith('"""', index)) {
        stringMode = "multiline-basic";
        index += 3;
        continue;
      }
      if (sourceLine.startsWith("'''", index)) {
        stringMode = "multiline-literal";
        index += 3;
        continue;
      }

      if (character === '"') {
        code[index] = '"';
        let stringEnd = index + 1;
        let closed = false;
        while (stringEnd < sourceLine.length) {
          if (sourceLine[stringEnd] === "\\") {
            stringEnd += 2;
          } else if (sourceLine[stringEnd] === '"') {
            stringEnd += 1;
            closed = true;
            break;
          } else {
            stringEnd += 1;
          }
        }
        if (!closed) {
          throw new Error("backend/Cargo.toml has an unterminated string.");
        }
        code[stringEnd - 1] = '"';
        index = stringEnd;
        continue;
      }

      if (character === "'") {
        const stringEnd = sourceLine.indexOf("'", index + 1);
        if (stringEnd < 0) {
          throw new Error("backend/Cargo.toml has an unterminated string.");
        }
        code[index] = "'";
        code[stringEnd] = "'";
        index = stringEnd + 1;
        continue;
      }

      code[index] = character;
      index += 1;
    }

    lines.push({ code: code.join(""), source: sourceLine });
  }

  if (stringMode !== "normal") {
    throw new Error("backend/Cargo.toml has an unterminated multiline string.");
  }
  return lines;
}

function cargoPackageVersion(source) {
  let foundPackage = false;
  let inPackage = false;

  for (const line of tomlCodeLines(source)) {
    const table = /^\s*(\[\[?[^\]\r\n]+\]\]?)\s*$/.exec(line.code);
    if (table) {
      if (table[1] === "[package]") {
        foundPackage = true;
        inPackage = true;
      } else if (inPackage) {
        break;
      }
      continue;
    }

    if (!inPackage) continue;
    const versionPrefix = /^\s*version\s*=\s*/.exec(line.code);
    if (!versionPrefix) continue;
    const versionMatch = /^"([^"\r\n]+)"\s*(?:#.*)?$/.exec(
      line.source.slice(versionPrefix[0].length),
    );
    if (versionMatch && VERSION_PATTERN.test(versionMatch[1])) {
      return versionMatch[1];
    }
    break;
  }

  if (!foundPackage) {
    throw new Error("backend/Cargo.toml has no [package] section.");
  }
  throw new Error("backend/Cargo.toml has no valid stable package version.");
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
