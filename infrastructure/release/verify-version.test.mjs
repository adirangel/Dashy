import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

import {
  readReleaseVersions,
  validateReleaseVersions,
} from "./verify-version.mjs";

const matching = {
  tauri: "0.1.0",
  cargo: "0.1.0",
  frontend: "0.1.0",
  frontendLock: "0.1.0",
  frontendLockRoot: "0.1.0",
};
const scriptPath = path.resolve("infrastructure/release/verify-version.mjs");
const workflowPath = path.resolve(".github/workflows/release-windows.yml");

async function readWorkflow() {
  return (await readFile(workflowPath, "utf8")).replace(/\r\n?/g, "\n");
}

function extractJob(source, jobName) {
  source = source.replace(/\r\n?/g, "\n");
  const marker = `  ${jobName}:`;
  const start = source.indexOf(`${marker}\n`);
  assert.notEqual(start, -1, `workflow job ${jobName} is missing`);
  const tail = source.slice(start + marker.length + 1);
  const nextJob = /^  [A-Za-z0-9_-]+:\s*$/m.exec(tail);
  return nextJob ? tail.slice(0, nextJob.index) : tail;
}

function extractRunScript(source, stepName) {
  source = source.replace(/\r\n?/g, "\n");
  const marker = `      - name: ${stepName}`;
  const start = source.indexOf(marker);
  assert.notEqual(start, -1, `workflow step ${stepName} is missing`);
  const tail = source.slice(start + marker.length);
  const nextStep = /^      - name: /m.exec(tail);
  const step = nextStep ? tail.slice(0, nextStep.index) : tail;
  const runMarker = "        run: |\n";
  const runStart = step.indexOf(runMarker);
  assert.notEqual(runStart, -1, `workflow step ${stepName} has no block script`);
  const lines = step.slice(runStart + runMarker.length).split("\n");
  const scriptLines = [];
  for (const line of lines) {
    if (line === "" || line.startsWith("          ")) {
      scriptLines.push(line.startsWith("          ") ? line.slice(10) : line);
      continue;
    }
    break;
  }
  return `${scriptLines.join("\n").trimEnd()}\n`;
}

async function runPowerShell(script, env = {}) {
  const root = await mkdtemp(path.join(tmpdir(), "dashy-release-pwsh-"));
  const sourcePath = path.join(root, "step.ps1");
  await writeFile(sourcePath, script);
  const result = spawnSync(
    "pwsh",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-File", sourcePath],
    {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, ...env },
    },
  );
  return { ...result, root };
}

function sha256(content) {
  return createHash("sha256").update(content).digest("hex");
}

const releaseNotes = [
  "Private Windows x64 test build.",
  "",
  "This MSI is not code-signed. Windows may show an Unknown publisher or SmartScreen warning.",
  "Complete the Windows release checklist before publishing this draft.",
].join("\n");

function releaseRecord({ assets, draft = true, body = releaseNotes } = {}) {
  return JSON.stringify({
    isDraft: draft,
    isPrerelease: false,
    name: "Dashy v0.1.0",
    body,
    assets: assets ?? [],
  });
}

async function runReleaseScenario(script, scenario, { initialRelease, corruptHash = false } = {}) {
  const root = await mkdtemp(path.join(tmpdir(), `dashy-release-${scenario}-`));
  const assetDirectory = path.join(root, "assets");
  const logPath = path.join(root, "gh.log");
  await mkdir(assetDirectory);
  const msiName = "Dashy_0.1.0_x64_en-US.msi";
  const msiContent = Buffer.from("verified-release-msi");
  const msiHash = sha256(msiContent);
  const checksumContent = Buffer.from(`${msiHash}  ${msiName}`);
  const checksumHash = sha256(checksumContent);
  await writeFile(path.join(assetDirectory, msiName), msiContent);
  await writeFile(path.join(assetDirectory, `${msiName}.sha256`), checksumContent);

  const fullAssets = [
    { name: msiName, size: msiContent.length, digest: `sha256:${msiHash}` },
    {
      name: `${msiName}.sha256`,
      size: checksumContent.length,
      digest: `sha256:${checksumHash}`,
    },
  ];
  const finalRelease = releaseRecord({ assets: fullAssets });
  const mockPrelude = String.raw`
$script:mockViewCount = 0
$script:mockTagCheckCount = 0
function gh {
  $call = @($args) -join '|'
  Add-Content -LiteralPath $env:MOCK_GH_LOG -Value $call
  if ($args[0] -ceq 'release' -and $args[1] -ceq 'view') {
    $script:mockViewCount += 1
    if ($script:mockViewCount -eq 1) {
      if ($env:MOCK_GH_SCENARIO -ceq 'new' -or
          $env:MOCK_GH_SCENARIO -ceq 'network-error' -or
          $env:MOCK_GH_SCENARIO -ceq 'moved-before-mutation') {
        $global:LASTEXITCODE = 1
        return
      }
      Write-Output $env:MOCK_INITIAL_RELEASE
      $global:LASTEXITCODE = 0
      return
    }
    Write-Output $env:MOCK_FINAL_RELEASE
    $global:LASTEXITCODE = 0
    return
  }
  if ($args[0] -ceq 'api') {
    if ($args[-1] -match '/commits/') {
      $script:mockTagCheckCount += 1
      $isExactTagRef = $args[-1] -ceq 'repos/owner/Dashy/commits/refs/tags/v0.1.0'
      if (($env:MOCK_GH_SCENARIO -ceq 'missing-tag' -and $isExactTagRef) -or
          ($env:MOCK_GH_SCENARIO -ceq 'branch-only' -and $isExactTagRef)) {
        Write-Output 'gh: Not Found (HTTP 404)'
        $global:LASTEXITCODE = 1
        return
      }
      $sha = $env:EXPECTED_RELEASE_SHA
      if (($env:MOCK_GH_SCENARIO -ceq 'moved-before-mutation' -and $script:mockTagCheckCount -ge 2) -or
          ($env:MOCK_GH_SCENARIO -ceq 'moved-before-final' -and $script:mockTagCheckCount -ge 2)) {
        $sha = 'cccccccccccccccccccccccccccccccccccccccc'
      }
      Write-Output (@{ sha = $sha } | ConvertTo-Json -Compress)
      $global:LASTEXITCODE = 0
      return
    }
    if ($env:MOCK_GH_SCENARIO -ceq 'new' -or $env:MOCK_GH_SCENARIO -ceq 'moved-before-mutation') {
      Write-Output 'gh: Not Found (HTTP 404)'
    } else {
      Write-Output 'gh: service unavailable (HTTP 503)'
    }
    $global:LASTEXITCODE = 1
    return
  }
  if ($args[0] -ceq 'release' -and ($args[1] -ceq 'create' -or $args[1] -ceq 'upload')) {
    $global:LASTEXITCODE = 0
    return
  }
  $global:LASTEXITCODE = 12
}
`;
  const result = await runPowerShell(`${mockPrelude}\n${script}`, {
    MOCK_GH_LOG: logPath,
    MOCK_GH_SCENARIO: scenario,
    MOCK_INITIAL_RELEASE: initialRelease ?? finalRelease,
    MOCK_FINAL_RELEASE: finalRelease,
    RELEASE_TAG: "v0.1.0",
    RELEASE_REPOSITORY: "owner/Dashy",
    EXPECTED_RELEASE_SHA: "b".repeat(40),
    GITHUB_REPOSITORY: "owner/Dashy",
    RELEASE_ASSET_DIRECTORY: assetDirectory,
    BUILD_ARTIFACT_DIGEST: "a".repeat(64),
    BUILD_MSI_NAME: msiName,
    BUILD_CHECKSUM_NAME: `${msiName}.sha256`,
    BUILD_MSI_SIZE: String(msiContent.length),
    BUILD_CHECKSUM_SIZE: String(checksumContent.length),
    BUILD_MSI_SHA256: corruptHash ? "0".repeat(64) : msiHash,
    BUILD_CHECKSUM_SHA256: checksumHash,
    RELEASE_NOTES: releaseNotes,
  });
  let log = "";
  try {
    log = await readFile(logPath, "utf8");
  } catch {
    // Pre-GitHub validation failures intentionally produce no command log.
  }
  return { result, log, fullAssets };
}

async function createManifests({
  tauri = JSON.stringify({ version: "0.1.0" }),
  cargo = '[package]\nname = "dashy"\nversion = "0.1.0"\n',
  frontend = JSON.stringify({ version: "0.1.0" }),
  frontendLock = JSON.stringify({
    name: "dashy-frontend",
    version: "0.1.0",
    lockfileVersion: 3,
    packages: { "": { name: "dashy-frontend", version: "0.1.0" } },
  }),
} = {}) {
  const root = await mkdtemp(path.join(tmpdir(), "dashy-release-version-"));
  await mkdir(path.join(root, "backend"), { recursive: true });
  await mkdir(path.join(root, "frontend"), { recursive: true });
  await writeFile(path.join(root, "backend", "tauri.conf.json"), tauri);
  await writeFile(path.join(root, "backend", "Cargo.toml"), cargo);
  await writeFile(path.join(root, "frontend", "package.json"), frontend);
  await writeFile(path.join(root, "frontend", "package-lock.json"), frontendLock);
  return root;
}

test("workflow verification accepts a Windows CRLF checkout", async () => {
  const source = (await readWorkflow()).replace(/\n/g, "\r\n");
  assert.match(extractJob(source, "build-windows"), /contents: read/);
  assert.match(
    extractRunScript(source, "Require release commit on origin/main"),
    /merge-base --is-ancestor/,
  );
});

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
        frontendLock: "0.4.0",
        frontendLockRoot: "0.5.0",
      }),
    /backend\/Cargo\.toml=0\.1\.0.*frontend\/package\.json=0\.3\.0.*package-lock\.json#version=0\.4\.0.*packages\[""\]\.version=0\.5\.0/,
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

test("ignores version and table decoys inside Cargo multiline basic strings", async () => {
  const root = await createManifests({
    cargo: [
      '[package]',
      'name = "dashy"',
      '# version = "7.7.7"',
      '# [dependencies.comment-decoy]',
      'description = """',
      // TOML represents three content quotes as two quotes plus an escaped quote.
      'A quoted marker: ""\\"',
      'version = "9.9.9"',
      '[dependencies.decoy]',
      '"""',
      'version = "0.1.0"',
      '',
      '[dependencies]',
      'example = "1.0.0"',
      '',
    ].join("\n"),
  });

  assert.deepEqual(readReleaseVersions(root), matching);
});

test("ignores version and table decoys inside Cargo multiline literal strings", async () => {
  const root = await createManifests({
    cargo: [
      '[package]',
      'name = "dashy"',
      '# version = "7.7.7"',
      '# [dependencies.comment-decoy]',
      "description = '''",
      'version = "8.8.8"',
      '[workspace.package]',
      "'''",
      'version = "0.1.0"',
      '',
      '[dependencies]',
      'example = "1.0.0"',
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

  const unterminated = await createManifests({
    cargo: '[package]\ndescription = """never closed\nversion = "0.1.0"\n',
  });
  assert.throws(
    () => readReleaseVersions(unterminated),
    /Cargo\.toml.*unterminated multiline string/,
  );
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

test("rejects malformed frontend lockfiles and either invalid lockfile version field", async () => {
  const malformed = await createManifests({ frontendLock: "{" });
  assert.throws(
    () => readReleaseVersions(malformed),
    /frontend\/package-lock\.json.*valid JSON/,
  );

  const missingTopLevel = await createManifests({
    frontendLock: JSON.stringify({
      lockfileVersion: 3,
      packages: { "": { version: "0.1.0" } },
    }),
  });
  assert.throws(
    () => readReleaseVersions(missingTopLevel),
    /package-lock\.json#version.*valid version/,
  );

  const missingRootPackage = await createManifests({
    frontendLock: JSON.stringify({ version: "0.1.0", lockfileVersion: 3, packages: {} }),
  });
  assert.throws(
    () => readReleaseVersions(missingRootPackage),
    /package-lock\.json#packages\[""\]\.version.*valid version/,
  );

  const numericRootPackage = await createManifests({
    frontendLock: JSON.stringify({
      version: "0.1.0",
      lockfileVersion: 3,
      packages: { "": { version: 1 } },
    }),
  });
  assert.throws(
    () => readReleaseVersions(numericRootPackage),
    /package-lock\.json#packages\[""\]\.version.*valid version/,
  );
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
  assert.match(result.stderr, /frontend\/package-lock\.json#version=0\.1\.0/);
  assert.match(result.stderr, /frontend\/package-lock\.json#packages\[""\]\.version=0\.1\.0/);
  assert.doesNotMatch(result.stderr, new RegExp(root.replaceAll("\\", "\\\\")));
});

test("workflow isolates repository execution from the write-scoped release job", async () => {
  const source = await readWorkflow();
  const build = extractJob(source, "build-windows");
  const release = extractJob(source, "release-windows");

  assert.match(source, /^permissions: \{\}$/m);
  assert.match(build, /^    permissions:\n      contents: read$/m);
  assert.match(release, /^    needs: build-windows$/m);
  assert.match(release, /^    permissions:\n      contents: write$/m);
  assert.match(release, /RELEASE_REPOSITORY: \$\{\{ github\.repository \}\}/);
  assert.match(release, /EXPECTED_RELEASE_SHA: \$\{\{ github\.sha \}\}/);
  assert.match(release, /"repos\/\$repo\/commits\/refs\/tags\/\$tag"/);
  assert.doesNotMatch(release, /"repos\/\$repo\/commits\/\$tag"/);
  assert.ok(
    build.indexOf("name: Require release commit on origin/main")
      < build.indexOf("name: Install Node.js"),
    "ancestry must be established before dependency setup",
  );
  assert.match(build, /actions\/checkout@[0-9a-f]{40}/);
  assert.match(build, /persist-credentials: false/);
  assert.match(build, /git[^\n]*fetch[\s\S]*--unshallow[\s\S]*refs\/heads\/main/);
  assert.match(
    build,
    /actions\/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4\.6\.2/,
  );
  assert.match(
    build,
    /path: \|\n\s+\$\{\{ steps\.checksum\.outputs\.msi_path \}\}\n\s+\$\{\{ steps\.checksum\.outputs\.checksum_path \}\}/,
  );
  assert.match(
    release,
    /actions\/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4\.3\.0/,
  );
  assert.doesNotMatch(build, /secrets\.|contents: write/);
  assert.doesNotMatch(release, /actions\/checkout|setup-node|rust-toolchain|tauri-action/);
  assert.doesNotMatch(release, /(?:^|\s)(?:npm|node|cargo|git)\s|Invoke-Expression|Start-Process/im);
  assert.doesNotMatch(release, /--clobber|release\s+delete|asset\s+delete|Remove-Item/i);
  assert.equal((release.match(/^      - name:/gm) ?? []).length, 2);
  assert.equal((release.match(/^        run: \|$/gm) ?? []).length, 1);
  assert.equal((source.match(/secrets\.GITHUB_TOKEN/g) ?? []).length, 1);

  const uses = [...source.matchAll(/^\s+uses:\s+([^\s#]+)(?:\s+#.*)?$/gm)]
    .map((match) => match[1]);
  assert.equal(uses.length, 6);
  for (const action of uses) {
    assert.match(action, /^[^@]+@[0-9a-f]{40}$/, action);
  }
});

test("all release workflow PowerShell steps parse before execution", async () => {
  const source = await readWorkflow();
  const stepNames = [
    "Require release commit on origin/main",
    "Create MSI checksum and release payload",
    "Create or verify draft release",
  ];

  for (const stepName of stepNames) {
    const script = extractRunScript(source, stepName);
    const root = await mkdtemp(path.join(tmpdir(), "dashy-release-ast-"));
    const sourcePath = path.join(root, "step.ps1");
    await writeFile(sourcePath, script);
    const result = spawnSync(
      "pwsh",
      [
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "$tokens=$null;$errors=$null;[Management.Automation.Language.Parser]::ParseFile($env:SOURCE_PATH,[ref]$tokens,[ref]$errors)>$null;if($errors.Count){$errors|ForEach-Object ToString;exit 1}",
      ],
      { encoding: "utf8", env: { ...process.env, SOURCE_PATH: sourcePath } },
    );
    assert.equal(result.status, 0, `${stepName}: ${result.stderr || result.stdout}`);
  }
});

test("ancestry gate rejects fetch failures and tags outside origin/main", async () => {
  const source = await readWorkflow();
  const script = extractRunScript(source, "Require release commit on origin/main");
  const root = await mkdtemp(path.join(tmpdir(), "dashy-release-git-mock-"));
  const gitPath = path.join(root, "git.cmd");
  const logPath = path.join(root, "git.log");
  await writeFile(
    gitPath,
    [
      "@echo off",
      "echo %*>>\"%MOCK_GIT_LOG%\"",
      "if \"%1\"==\"-c\" if \"%3\"==\"fetch\" goto fetch",
      "if \"%1\"==\"fetch\" goto fetch",
      "if \"%1\"==\"rev-parse\" goto revparse",
      "if \"%1\"==\"merge-base\" goto mergebase",
      "exit /b 10",
      ":fetch",
      "if \"%MOCK_GIT_SCENARIO%\"==\"fetch-error\" exit /b 7",
      "exit /b 0",
      ":revparse",
      "if \"%MOCK_GIT_SCENARIO%\"==\"resolve-error\" exit /b 8",
      "if \"%3\"==\"refs/remotes/origin/main{commit}\" echo bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "if not \"%3\"==\"refs/remotes/origin/main{commit}\" echo aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "exit /b 0",
      ":mergebase",
      "if \"%MOCK_GIT_SCENARIO%\"==\"not-ancestor\" exit /b 1",
      "if \"%MOCK_GIT_SCENARIO%\"==\"merge-error\" exit /b 9",
      "exit /b 0",
      "",
    ].join("\r\n"),
  );
  const baseEnv = {
    PATH: `${root};${process.env.PATH}`,
    Path: `${root};${process.env.Path ?? process.env.PATH}`,
    MOCK_GIT_LOG: logPath,
    GITHUB_SHA: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  };

  const success = await runPowerShell(script, {
    ...baseEnv,
    MOCK_GIT_SCENARIO: "success",
  });
  assert.equal(success.status, 0, success.stderr || success.stdout);

  const outsideMain = await runPowerShell(script, {
    ...baseEnv,
    MOCK_GIT_SCENARIO: "not-ancestor",
  });
  assert.notEqual(outsideMain.status, 0);
  assert.match(outsideMain.stderr, /not an ancestor of origin\/main/i);

  const fetchFailure = await runPowerShell(script, {
    ...baseEnv,
    MOCK_GIT_SCENARIO: "fetch-error",
  });
  assert.notEqual(fetchFailure.status, 0);
  assert.match(fetchFailure.stderr, /fetch origin\/main/i);

  const ancestryFailure = await runPowerShell(script, {
    ...baseEnv,
    MOCK_GIT_SCENARIO: "merge-error",
  });
  assert.notEqual(ancestryFailure.status, 0);
  assert.match(ancestryFailure.stderr, /Could not verify release ancestry/i);
});

test("checksum step stages exactly one MSI and its matching digest", async () => {
  const source = await readWorkflow();
  const script = extractRunScript(source, "Create MSI checksum and release payload");
  const fixture = await mkdtemp(path.join(tmpdir(), "dashy-release-msi-"));
  const msiPath = path.join(fixture, "Dashy_0.1.0_x64_en-US.msi");
  const outputPath = path.join(fixture, "outputs.txt");
  await writeFile(msiPath, "verified-msi-payload");

  const success = await runPowerShell(script, {
    ARTIFACT_PATHS: JSON.stringify([msiPath]),
    RUNNER_TEMP: fixture,
    GITHUB_OUTPUT: outputPath,
  });
  assert.equal(success.status, 0, success.stderr || success.stdout);
  const releaseDir = path.join(fixture, "dashy-release-assets");
  const files = await readdir(releaseDir);
  assert.deepEqual(files.sort(), [
    "Dashy_0.1.0_x64_en-US.msi",
    "Dashy_0.1.0_x64_en-US.msi.sha256",
  ]);
  const checksum = await readFile(`${path.join(releaseDir, "Dashy_0.1.0_x64_en-US.msi")}.sha256`, "utf8");
  assert.match(checksum, /^[0-9a-f]{64}  Dashy_0\.1\.0_x64_en-US\.msi$/);
  const outputs = await readFile(outputPath, "utf8");
  assert.match(outputs, /^msi_name=Dashy_0\.1\.0_x64_en-US\.msi$/m);
  assert.match(outputs, /^checksum_name=Dashy_0\.1\.0_x64_en-US\.msi\.sha256$/m);
  assert.match(outputs, /^msi_sha256=[0-9a-f]{64}$/m);
  assert.match(outputs, /^checksum_sha256=[0-9a-f]{64}$/m);

  const missing = await runPowerShell(script, {
    ARTIFACT_PATHS: "[]",
    RUNNER_TEMP: path.join(fixture, "missing"),
    GITHUB_OUTPUT: path.join(fixture, "missing-output.txt"),
  });
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /Expected one MSI artifact, found 0/);

  const secondMsi = path.join(fixture, "Dashy_0.1.1_x64_en-US.msi");
  await writeFile(secondMsi, "second-msi");
  const multiple = await runPowerShell(script, {
    ARTIFACT_PATHS: JSON.stringify([msiPath, secondMsi]),
    RUNNER_TEMP: path.join(fixture, "multiple"),
    GITHUB_OUTPUT: path.join(fixture, "multiple-output.txt"),
  });
  assert.notEqual(multiple.status, 0);
  assert.match(multiple.stderr, /Expected one MSI artifact, found 2/);
});

test("release step mutates only a verified draft with the exact transferred payload", async () => {
  const source = await readWorkflow();
  const script = extractRunScript(source, "Create or verify draft release");

  const created = await runReleaseScenario(script, "new");
  assert.equal(created.result.status, 0, created.result.stderr || created.result.stdout);
  assert.match(created.log, /^api\|.*repos\/owner\/Dashy\/commits\/refs\/tags\/v0\.1\.0$/m);
  assert.match(created.log, /^release\|create\|/m);
  assert.doesNotMatch(created.log, /^release\|upload\|/m);

  const complete = await runReleaseScenario(script, "complete");
  assert.equal(complete.result.status, 0, complete.result.stderr || complete.result.stdout);
  assert.doesNotMatch(complete.log, /^release\|(create|upload)\|/m);

  const partial = await runReleaseScenario(script, "partial", {
    initialRelease: releaseRecord({ assets: [created.fullAssets[0]] }),
  });
  assert.equal(partial.result.status, 0, partial.result.stderr || partial.result.stdout);
  assert.match(partial.log, /^release\|upload\|/m);
  assert.doesNotMatch(partial.log, /^release\|create\|/m);

  const published = await runReleaseScenario(script, "published", {
    initialRelease: releaseRecord({ assets: [], draft: false }),
  });
  assert.notEqual(published.result.status, 0);
  assert.match(published.result.stderr, /Refusing to modify a non-draft release/);
  assert.doesNotMatch(published.log, /^release\|(create|upload)\|/m);

  const digestMismatch = await runReleaseScenario(script, "digest-mismatch", {
    initialRelease: releaseRecord({
      assets: [{ ...created.fullAssets[0], digest: `sha256:${"f".repeat(64)}` }],
    }),
  });
  assert.notEqual(digestMismatch.result.status, 0);
  assert.match(digestMismatch.result.stderr, /asset digest does not match/);
  assert.doesNotMatch(digestMismatch.log, /^release\|(create|upload)\|/m);

  const companionAssets = await runReleaseScenario(script, "companion-assets", {
    initialRelease: releaseRecord({
      assets: [
        ...created.fullAssets,
        { name: "Dashy_0.1.0_universal.dmg", size: 5, digest: `sha256:${"d".repeat(64)}` },
        { name: "Dashy_0.1.0_universal.dmg.sha256", size: 5, digest: `sha256:${"d".repeat(64)}` },
        { name: "Dashy_0.1.0_amd64.deb", size: 5, digest: `sha256:${"d".repeat(64)}` },
        { name: "Dashy-0.1.0-1.x86_64.rpm", size: 5, digest: `sha256:${"d".repeat(64)}` },
        { name: "Dashy_0.1.0_amd64.AppImage.sha256", size: 5, digest: `sha256:${"d".repeat(64)}` },
      ],
    }),
  });
  assert.equal(companionAssets.result.status, 0, companionAssets.result.stderr || companionAssets.result.stdout);
  assert.doesNotMatch(companionAssets.log, /^release\|(create|upload)\|/m);

  const unexpectedAsset = await runReleaseScenario(script, "unexpected-asset", {
    initialRelease: releaseRecord({
      assets: [{ name: "extra.exe", size: 1, digest: `sha256:${"e".repeat(64)}` }],
    }),
  });
  assert.notEqual(unexpectedAsset.result.status, 0);
  assert.match(unexpectedAsset.result.stderr, /unexpected assets: extra\.exe/);
  assert.doesNotMatch(unexpectedAsset.log, /^release\|(create|upload)\|/m);

  const networkFailure = await runReleaseScenario(script, "network-error");
  assert.notEqual(networkFailure.result.status, 0);
  assert.match(networkFailure.result.stderr, /safely determine whether the release exists/);
  assert.doesNotMatch(networkFailure.log, /^release\|(create|upload)\|/m);

  const missingTag = await runReleaseScenario(script, "missing-tag");
  assert.notEqual(missingTag.result.status, 0);
  assert.match(missingTag.result.stderr, /Could not resolve the live release tag/i);
  assert.doesNotMatch(missingTag.log, /^release\|(create|upload)\|/m);

  const branchOnly = await runReleaseScenario(script, "branch-only");
  assert.notEqual(branchOnly.result.status, 0);
  assert.match(branchOnly.result.stderr, /Could not resolve the live release tag/i);
  assert.doesNotMatch(branchOnly.log, /^release\|(create|upload)\|/m);

  const movedBeforeMutation = await runReleaseScenario(script, "moved-before-mutation");
  assert.notEqual(movedBeforeMutation.result.status, 0);
  assert.match(movedBeforeMutation.result.stderr, /no longer points to the triggering commit/i);
  assert.doesNotMatch(movedBeforeMutation.log, /^release\|(create|upload)\|/m);

  const movedBeforeFinal = await runReleaseScenario(script, "moved-before-final");
  assert.notEqual(movedBeforeFinal.result.status, 0);
  assert.match(movedBeforeFinal.result.stderr, /no longer points to the triggering commit/i);
  assert.doesNotMatch(movedBeforeFinal.log, /^release\|(create|upload)\|/m);

  const corruptTransfer = await runReleaseScenario(script, "complete", {
    corruptHash: true,
  });
  assert.notEqual(corruptTransfer.result.status, 0);
  assert.match(corruptTransfer.result.stderr, /payload digests do not match/);
  assert.equal(corruptTransfer.log, "");
});

test("macOS and Linux release workflow only extends the draft the Windows workflow created", async () => {
  const desktopPath = path.resolve(".github/workflows/release-desktop.yml");
  const source = (await readFile(desktopPath, "utf8")).replace(/\r\n?/g, "\n");
  const gate = extractJob(source, "gate");
  const macos = extractJob(source, "build-macos");
  const linux = extractJob(source, "build-linux");
  const release = extractJob(source, "release-packages");

  assert.match(source, /^permissions: \{\}$/m);
  // A tag push and a parameterless manual dispatch on that tag are the only
  // entry points, and every checkout builds the commit the run was started
  // for. A ref named by a run input would count as untrusted code executed in
  // the default-branch context (CodeQL's cache-poisoning rules).
  assert.match(source, /^on:\n  push:\n    tags:\n      - "v\[0-9\]\*\.\[0-9\]\*\.\[0-9\]\*"\n  workflow_dispatch:\n\npermissions/m);
  assert.doesNotMatch(source, /workflow_run|pull_request_target|github\.event\.workflow_run|inputs\./);
  assert.equal((source.match(/ref: \$\{\{ github\.sha \}\}/g) ?? []).length, 3);
  assert.doesNotMatch(source, /ref: \$\{\{ (steps|needs)\./);
  assert.match(gate, /"repos\/\$RELEASE_REPOSITORY\/commits\/refs\/tags\/\$tag"/);
  assert.match(gate, /REF_TYPE" != "tag"/);
  assert.match(gate, /no longer points to the commit this run was started for/);
  assert.match(gate, /^    permissions:\n      contents: read$/m);
  assert.match(gate, /"repos\/\$RELEASE_REPOSITORY\/commits\/refs\/tags\/\$RELEASE_TAG"/);
  assert.match(gate, /merge-base --is-ancestor/);
  assert.match(gate, /verify-version\.mjs "\$RELEASE_TAG"/);
  for (const build of [macos, linux]) {
    assert.match(build, /^    needs: gate$/m);
    assert.match(build, /^    permissions:\n      contents: read$/m);
    assert.match(build, /ref: \$\{\{ github\.sha \}\}/);
    assert.match(build, /persist-credentials: false/);
    assert.match(build, /infrastructure\/release\/stage-assets\.sh/);
    assert.match(build, /-- --locked/);
    assert.doesNotMatch(build, /secrets\.|contents: write/);
  }
  assert.match(macos, /--bundles dmg --target universal-apple-darwin/);
  assert.match(linux, /--bundles deb,rpm,appimage --target x86_64-unknown-linux-gnu/);
  assert.match(release, /^    needs: \[gate, build-macos, build-linux\]$/m);
  assert.match(release, /^    permissions:\n      contents: write$/m);
  assert.match(release, /Refusing to modify a non-draft release/);
  assert.match(release, /has not created the draft/);
  assert.match(release, /for attempt in \$\(seq 1 60\)/);
  assert.match(release, /sleep 60/);
  assert.doesNotMatch(release, /actions\/checkout|setup-node|rust-toolchain|tauri-action/);
  assert.doesNotMatch(release, /release\s+create|--clobber|release\s+delete|asset\s+delete/);
  assert.equal((source.match(/secrets\.GITHUB_TOKEN/g) ?? []).length, 1);

  const uses = [...source.matchAll(/^\s+uses:\s+([^\s#]+)(?:\s+#.*)?$/gm)]
    .map((match) => match[1]);
  assert.ok(uses.length >= 8);
  for (const action of uses) {
    assert.match(action, /^[^@]+@[0-9a-f]{40}$/, action);
  }
});

test("asset staging script rejects unexpected bundles and stages one checksum per bundle", async () => {
  const scriptPath = path.resolve("infrastructure/release/stage-assets.sh");
  // The macOS package builder runs this script on the runner's Bash 3.2, which
  // has no mapfile or readarray, and on a system without sha256sum.
  const script = await readFile(scriptPath, "utf8");
  assert.doesNotMatch(script, /\b(mapfile|readarray)\b/);
  assert.match(script, /shasum -a 256/);
  const root = await mkdtemp(path.join(tmpdir(), "dashy-release-stage-"));
  const bundleDirectory = path.join(root, "bundle");
  await mkdir(bundleDirectory);
  const dmg = path.join(bundleDirectory, "Dashy_0.1.0_universal.dmg");
  await writeFile(dmg, "verified-dmg");
  await mkdir(path.join(bundleDirectory, "Dashy.app"));

  const run = (runnerTemp, artifacts, extraEnv = {}) => spawnSync("bash", [scriptPath], {
    encoding: "utf8",
    env: {
      ...process.env,
      ARTIFACT_PATHS: JSON.stringify(artifacts),
      RELEASE_VERSION: "0.1.0",
      EXPECTED_PATTERN: String.raw`^Dashy_[0-9.]+_universal\.dmg$`,
      EXPECTED_COUNT: "1",
      RUNNER_TEMP: runnerTemp,
      ...extraEnv,
    },
  });

  if (process.platform === "win32") {
    return; // the staging script runs on the macOS and Linux build runners only
  }

  const success = run(path.join(root, "ok"), [dmg, path.join(bundleDirectory, "Dashy.app")]);
  assert.equal(success.status, 0, success.stderr || success.stdout);
  const staged = await readdir(path.join(root, "ok", "dashy-release-assets"));
  assert.deepEqual(staged.sort(), [
    "Dashy_0.1.0_universal.dmg",
    "Dashy_0.1.0_universal.dmg.sha256",
  ]);
  const checksum = await readFile(
    path.join(root, "ok", "dashy-release-assets", "Dashy_0.1.0_universal.dmg.sha256"),
    "utf8",
  );
  assert.equal(checksum, `${sha256("verified-dmg")}  Dashy_0.1.0_universal.dmg\n`);

  const stray = path.join(bundleDirectory, "Dashy_0.1.0_x64.exe");
  await writeFile(stray, "stray");
  const rejected = run(path.join(root, "stray"), [dmg, stray]);
  assert.notEqual(rejected.status, 0);
  assert.match(rejected.stderr, /Unexpected bundle name: Dashy_0\.1\.0_x64\.exe/);

  const wrongVersion = path.join(bundleDirectory, "Dashy_0.2.0_universal.dmg");
  await writeFile(wrongVersion, "wrong");
  const mismatch = run(path.join(root, "version"), [wrongVersion]);
  assert.notEqual(mismatch.status, 0);
  assert.match(mismatch.stderr, /does not carry version 0\.1\.0/);

  const none = run(path.join(root, "none"), []);
  assert.notEqual(none.status, 0);
  assert.match(none.stderr, /Expected 1 bundles, staged 0/);
});
