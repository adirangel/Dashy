import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

test("provider PATH repair preserves saved entries and resolves in a fresh PowerShell", { skip: process.platform !== "win32" }, () => {
  const root = mkdtempSync(path.join(tmpdir(), "dashy-path-test-"));
  const script = path.join(root, "test.ps1");
  writeFileSync(script, "\ufeff" + String.raw`
$ErrorActionPreference = 'Stop'
$tokens = $null; $parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($env:DASHY_PATH_SCRIPT, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
# Import fixture-safe helpers only; never run the registry writer in a test.
$ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in @('Get-DashyUserPathUpdate', 'Find-DashyCommandDirectory', 'Repair-DashyCodexCommandAlias') }, $true) |
  ForEach-Object { Invoke-Expression $_.Extent.Text }
function Assert-Equal($actual, $expected) {
  if ($actual -cne $expected) { throw "Mismatch: expected [$expected], got [$actual]" }
}
Assert-Equal (Get-DashyUserPathUpdate '' '' 'C:\Tools') 'C:\Tools'
Assert-Equal (Get-DashyUserPathUpdate 'C:\Existing;' '' 'C:\Tools') 'C:\Existing;C:\Tools'
Assert-Equal (Get-DashyUserPathUpdate 'C:\TOOLS\' '' 'c:\tools') 'C:\TOOLS\'
Assert-Equal (Get-DashyUserPathUpdate 'C:\User' 'C:\Tools' 'C:\Tools') 'C:\User'
$env:DASHY_EXPANDED_ROOT = 'C:\Space and עברית'
Assert-Equal (Get-DashyUserPathUpdate '%DASHY_EXPANDED_ROOT%\bin' '' 'C:\Space and עברית\bin') '%DASHY_EXPANDED_ROOT%\bin'
$long = ('C:\Existing;' * 1000) + '%DASHY_EXPANDED_ROOT%'
Assert-Equal (Get-DashyUserPathUpdate $long '' 'C:\New') ($long + ';C:\New')
$bin = Join-Path $env:DASHY_TEST_ROOT 'Space and עברית'
New-Item -ItemType Directory -Path $bin | Out-Null
Set-Content -LiteralPath (Join-Path $bin 'codex.cmd') -Value '@echo test' -Encoding ASCII
Assert-Equal (Find-DashyCommandDirectory 'codex' @('C:\missing', $bin)) $bin
$failed = $false
try { Find-DashyCommandDirectory 'gh' @($bin) } catch { $failed = $true }
if (-not $failed) { throw 'Missing command must fail verification' }
# Reproduce a portable WinGet install with its target-qualified executable and
# no command alias. Copy only the system shell as an isolated executable fixture.
$portable = Join-Path $bin 'portable & !test!'
New-Item -ItemType Directory -Path $portable | Out-Null
Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $portable 'codex-x86_64-pc-windows-msvc.exe')
Assert-Equal (Find-DashyCommandDirectory 'codex' @($portable)) $portable
$launcher = Join-Path $portable 'codex.cmd'
$before = [IO.File]::ReadAllText($launcher)
& $launcher /d /c 'exit 17'
if ($LASTEXITCODE -ne 17) { throw 'Portable launcher lost the child exit code' }
Assert-Equal (Find-DashyCommandDirectory 'codex' @($portable)) $portable
Assert-Equal ([IO.File]::ReadAllText($launcher)) $before
# Existing shell commands take priority over creating a portable alias.
Remove-Item -LiteralPath $launcher
Assert-Equal (Find-DashyCommandDirectory 'codex' @($portable, $bin)) $bin
if (Test-Path -LiteralPath $launcher) { throw 'Existing command should prevent alias creation' }
Assert-Equal (Find-DashyCommandDirectory 'codex' @($portable)) $portable
$env:Path = Get-DashyUserPathUpdate '' '' $portable
& $env:DASHY_POWERSHELL -NoProfile -NonInteractive -Command 'codex /d /c "exit 17"; exit $LASTEXITCODE'
if ($LASTEXITCODE -ne 17) { throw 'Fresh shell could not run the repaired portable CLI' }
$env:Path = Get-DashyUserPathUpdate '' '' $bin
& $env:DASHY_POWERSHELL -NoProfile -NonInteractive -Command 'if (-not (Get-Command codex -CommandType Application -ErrorAction SilentlyContinue)) { exit 1 }'
if ($LASTEXITCODE -ne 0) { throw 'New PowerShell did not discover codex' }
`);
  const powershell = path.join(process.env.SystemRoot, "System32/WindowsPowerShell/v1.0/powershell.exe");
  try {
    const result = spawnSync(powershell, ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script], {
      encoding: "utf8", timeout: 30000,
      env: { ...process.env, DASHY_PATH_SCRIPT: path.resolve("backend/src/dashboard/windows_install_path.ps1"), DASHY_TEST_ROOT: root, DASHY_POWERSHELL: powershell },
    });
    assert.equal(result.status, 0, result.stderr || result.error?.message);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
