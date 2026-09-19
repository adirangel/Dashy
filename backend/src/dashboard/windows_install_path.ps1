# Embedded in Dashy; run only after an explicitly requested provider install.
# Never dot-source a user profile, invoke a provider, or copy a process PATH.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$dashyCommand = '__DASHY_COMMAND__'

function Get-DashyUserPathUpdate {
    param(
        [AllowEmptyString()][string]$UserPath,
        [AllowEmptyString()][string]$MachinePath,
        [Parameter(Mandatory)][string]$Directory
    )
    $normalizedDirectory = $Directory.TrimEnd('\', '/')
    foreach ($segment in ($MachinePath + ';' + $UserPath) -split ';') {
        $expanded = [Environment]::ExpandEnvironmentVariables($segment.Trim().Trim('"')).TrimEnd('\', '/')
        if ($expanded -ieq $normalizedDirectory) { return $UserPath }
    }
    if ([string]::IsNullOrEmpty($UserPath)) { return $Directory }
    $separator = if ($UserPath.EndsWith(';')) { '' } else { ';' }
    return $UserPath + $separator + $Directory
}

function Find-DashyCommandDirectory {
    param([string]$Name, [string[]]$Directories)
    foreach ($directory in $Directories) {
        if ([string]::IsNullOrWhiteSpace($directory)) { continue }
        $directory = [Environment]::ExpandEnvironmentVariables($directory.Trim().Trim('"'))
        if (-not [IO.Path]::IsPathRooted($directory)) { continue }
        foreach ($extension in @('.exe', '.cmd')) {
            if (Test-Path -LiteralPath (Join-Path $directory ($Name + $extension)) -PathType Leaf) {
                # Use the shell command directory, never a nested npm payload or
                # a bundled node.exe directory, which would shadow other tools.
                return $directory
            }
        }
    }
    if ($Name -eq 'codex') {
        return Repair-DashyCodexCommandAlias -Directories $Directories
    }
    throw 'The installer completed but the provider command is missing.'
}

function Repair-DashyCodexCommandAlias {
    param([string[]]$Directories)
    # WinGet may add the package directory without creating codex.exe. The
    # relative ASCII launcher supports Unicode paths and adjacent helpers.
    # Run only after an explicit install; never replace an existing command.
    foreach ($directory in $Directories) {
        if ([string]::IsNullOrWhiteSpace($directory)) { continue }
        $directory = [Environment]::ExpandEnvironmentVariables($directory.Trim().Trim('"'))
        if (-not [IO.Path]::IsPathRooted($directory)) { continue }
        foreach ($payload in @('codex-x86_64-pc-windows-msvc.exe', 'codex-aarch64-pc-windows-msvc.exe')) {
            if (-not (Test-Path -LiteralPath (Join-Path $directory $payload) -PathType Leaf)) { continue }
            $launcher = Join-Path $directory 'codex.cmd'
            $lines = @('@echo off', 'setlocal DisableDelayedExpansion', ('@"%~dp0' + $payload + '" %*'), 'exit /b %errorlevel%', '')
            $bytes = [Text.Encoding]::ASCII.GetBytes([string]::Join([Environment]::NewLine, $lines))
            $stream = [IO.File]::Open($launcher, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
            try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
            return $directory
        }
    }
    throw 'The installer completed but the Codex executable is missing.'
}

function Repair-DashyInstalledCommand {
    param([string]$Name)
    if ($Name -notin @('claude', 'codex', 'gh', 'grok', 'cursor-agent')) {
        throw 'Unsupported provider command.'
    }
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        # Preserve both the original text and expandable variables. Do not use
        # setx (truncation), the inherited process PATH, or a machine-wide write.
        $userPath = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        $pathKind = if ($key.GetValueNames() -contains 'Path') {
            $key.GetValueKind('Path')
        } else {
            [Microsoft.Win32.RegistryValueKind]::ExpandString
        }
        $machinePath = [string][Environment]::GetEnvironmentVariable('Path', 'Machine')
        $directories = @((($machinePath + ';' + $userPath) -split ';'))
        $directories += @(
            (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links'),
            (Join-Path $env:LOCALAPPDATA 'cursor-agent'),
            (Join-Path $env:LOCALAPPDATA 'Programs\OpenAI\Codex\bin'),
            (Join-Path $env:USERPROFILE '.local\bin'),
            (Join-Path $env:USERPROFILE '.grok\bin'),
            (Join-Path $env:APPDATA 'npm')
        )
        foreach ($root in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
            if ($root) {
                $directories += (Join-Path $root 'WinGet\Links')
                $directories += (Join-Path $root 'GitHub CLI')
            }
        }
        $packageRoots = @((Join-Path $env:LOCALAPPDATA 'Microsoft/WinGet/Packages'))
        foreach ($root in @($env:ProgramFiles, [Environment]::GetEnvironmentVariable('ProgramFiles(x86)'))) {
            if ($root) { $packageRoots += (Join-Path $root 'WinGet/Packages') }
        }
        foreach ($root in $packageRoots) {
            foreach ($package in @('OpenAI.Codex', 'Anthropic.ClaudeCode', 'xAI.GrokBuild')) {
                $directories += (Join-Path $root ($package + '_Microsoft.Winget.Source_8wekyb3d8bbwe'))
            }
        }
        $directory = Find-DashyCommandDirectory -Name $Name -Directories $directories
        $updated = Get-DashyUserPathUpdate -UserPath $userPath -MachinePath $machinePath -Directory $directory
        if ($updated -cne $userPath) {
            $key.SetValue('Path', $updated, $pathKind)
        }
        # Verify discovery from persisted values alone; stale inherited PATH
        # entries must not make an incomplete installation look successful.
        $env:Path = [Environment]::ExpandEnvironmentVariables($machinePath + ';' + $updated)
        $resolved = Get-Command -Name $Name -CommandType Application -ErrorAction Stop
        if (-not $resolved) { throw 'The provider command is not available on the persisted PATH.' }
    }
    finally { $key.Dispose() }

    # Notify Explorer even if WinGet already wrote PATH. New shells launched by
    # Explorer otherwise inherit its stale environment until the next sign-in.
    Add-Type -TypeDefinition '
using System;
using System.Runtime.InteropServices;
public static class DashyEnvironmentNotification {
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SendMessageTimeout(IntPtr window, uint message,
        UIntPtr wparam, string lparam, uint flags, uint timeout, out UIntPtr result);
}'
    $result = [UIntPtr]::Zero
    $sent = [DashyEnvironmentNotification]::SendMessageTimeout(
        [IntPtr]0xffff, 0x001a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
    if ($sent -eq [IntPtr]::Zero) { throw 'Windows did not acknowledge the environment update.' }
}

Repair-DashyInstalledCommand -Name $dashyCommand
