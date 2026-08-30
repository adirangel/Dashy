[CmdletBinding()]
param(
    [switch]$CheckOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requiredCommands = @(
    'winget',
    'git',
    'gh',
    'node',
    'npm',
    'rustc',
    'cargo',
    'cargo-tauri',
    'claude',
    'codex'
)
$dashyWingetNoApplicationsFoundExitCode = -1978335212
$dashyVisualStudioBuildToolsId = 'Microsoft.VisualStudio.2022.BuildTools'
$dashyVisualStudioBuildToolsProductId = 'Microsoft.VisualStudio.Product.BuildTools'
$dashyVisualStudioVCToolsWorkloadId = 'Microsoft.VisualStudio.Workload.VCTools'

$packageSpecs = @(
    [pscustomobject]@{ Id = 'Git.Git'; Commands = @('git'); Override = $null },
    [pscustomobject]@{ Id = 'GitHub.cli'; Commands = @('gh'); Override = $null },
    [pscustomobject]@{ Id = 'OpenJS.NodeJS.LTS'; Commands = @('node', 'npm'); Override = $null },
    [pscustomobject]@{ Id = 'Rustlang.Rustup'; Commands = @('rustc', 'cargo'); Override = $null },
    [pscustomobject]@{
        Id = 'Microsoft.VisualStudio.2022.BuildTools'
        Commands = @()
        Override = '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    },
    [pscustomobject]@{ Id = 'Microsoft.EdgeWebView2Runtime'; Commands = @(); Override = $null },
    [pscustomobject]@{ Id = 'Anthropic.ClaudeCode'; Commands = @('claude'); Override = $null },
    [pscustomobject]@{ Id = 'OpenAI.Codex'; Commands = @('codex'); Override = $null }
)

function Test-DashyCommand {
    param([Parameter(Mandatory)][string]$Name)

    return [bool](Get-Command -Name $Name -ErrorAction SilentlyContinue)
}

function Get-DashyMergedProcessPath {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$CurrentPath,
        [Parameter(Mandatory)][AllowEmptyString()][string]$MachinePath,
        [Parameter(Mandatory)][AllowEmptyString()][string]$UserPath
    )

    $seen = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    $segments = [System.Collections.Generic.List[string]]::new()
    foreach ($pathValue in @($CurrentPath, $MachinePath, $UserPath)) {
        foreach ($segment in $pathValue -split ';') {
            $normalized = $segment.Trim()
            if (-not [string]::IsNullOrWhiteSpace($normalized) -and $seen.Add($normalized)) {
                $segments.Add($normalized)
            }
        }
    }

    return $segments -join ';'
}

function Update-DashyProcessPath {
    $currentPath = $env:Path
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = Get-DashyMergedProcessPath -CurrentPath $currentPath -MachinePath $machinePath -UserPath $userPath
}

function Get-DashyWingetPackageState {
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][int]$ExitCode,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Output
    )

    $hasPlainExactId = $false
    foreach ($line in $Output -split "`r?`n") {
        if (@($line.Trim() -split '\s+' | Where-Object { $_ -ieq $Id }).Count -gt 0) {
            $hasPlainExactId = $true
            break
        }
    }

    if ($ExitCode -eq 0) {
        if ($hasPlainExactId) {
            return 'Installed'
        }

        throw "WinGet inventory lookup for '$Id' returned exit code 0 without an exact package ID. Dashy will not assume the package is absent."
    }

    if ($ExitCode -eq $dashyWingetNoApplicationsFoundExitCode) {
        return 'NotInstalled'
    }

    throw "WinGet inventory lookup for '$Id' failed (exit code $ExitCode). Check WinGet sources and retry; Dashy will not assume the package is absent."
}

function Test-DashyWingetPackage {
    param([Parameter(Mandatory)][string]$Id)

    $output = & winget list --id $Id --exact --accept-source-agreements 2>&1 | Out-String
    $state = Get-DashyWingetPackageState -Id $Id -ExitCode $LASTEXITCODE -Output $output

    return $state -eq 'Installed'
}

function Get-DashyPackageDecision {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$RequiredCommands,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$VisibleCommands,
        [Parameter(Mandatory)][bool]$PackageInstalled
    )

    if ($RequiredCommands.Count -eq 0) {
        if ($PackageInstalled) {
            return 'SkipInstalled'
        }

        return 'Install'
    }

    $missingCommands = @($RequiredCommands | Where-Object { $_ -notin $VisibleCommands })
    if ($RequiredCommands.Count -gt 0 -and $missingCommands.Count -eq 0) {
        return 'SkipAvailable'
    }

    if ($PackageInstalled) {
        return 'StopPartialInstall'
    }

    return 'Install'
}

function Get-DashyVsWhereInstallationPath {
    param(
        [Parameter(Mandatory)][int]$ExitCode,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Output
    )

    if ($ExitCode -ne 0) {
        throw "vswhere failed while probing Visual Studio Build Tools (exit code $ExitCode)."
    }

    $paths = @(
        $Output -split '\r?\n' |
            ForEach-Object { $_.Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    )
    if ($paths.Count -eq 0) {
        return $null
    }
    if ($paths.Count -ne 1) {
        throw 'vswhere returned more than one installation path; Dashy will not choose an instance automatically.'
    }

    $installationPath = $paths[0]
    if ($installationPath -notmatch '^[A-Za-z]:\\') {
        throw 'vswhere did not return a fully qualified local installation path; Dashy will not pass it to setup.exe.'
    }

    return $installationPath
}

function Get-DashyBuildToolsAction {
    param(
        [Parameter(Mandatory)][bool]$PackageInstalled,
        [Parameter(Mandatory)][bool]$WorkloadInstalled,
        [Parameter(Mandatory)][bool]$InstanceResolved,
        [Parameter(Mandatory)][bool]$InstallerAvailable
    )

    if (-not $PackageInstalled) {
        return 'InstallPackage'
    }
    if ($WorkloadInstalled) {
        return 'SkipAvailable'
    }
    if ($InstanceResolved -and $InstallerAvailable) {
        return 'ModifyExisting'
    }

    return 'StopRepair'
}

function Invoke-DashyVsWhereInstallationPath {
    param(
        [Parameter(Mandatory)][string]$VsWherePath,
        [switch]$RequiresVCTools
    )

    $arguments = @(
        '-latest',
        '-products', $dashyVisualStudioBuildToolsProductId
    )
    if ($RequiresVCTools) {
        $arguments += @('-requires', $dashyVisualStudioVCToolsWorkloadId)
    }
    $arguments += @('-property', 'installationPath')

    $output = & $VsWherePath @arguments 2>&1 | Out-String
    return Get-DashyVsWhereInstallationPath -ExitCode $LASTEXITCODE -Output $output
}

function Get-DashyBuildToolsProbe {
    $programFilesX86 = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFilesX86)
    if ([string]::IsNullOrWhiteSpace($programFilesX86)) {
        return [pscustomobject]@{
            InstallationPath = $null
            InstanceResolved = $false
            WorkloadInstalled = $false
            InstallerPath = $null
            InstallerAvailable = $false
        }
    }

    $installerRoot = Join-Path $programFilesX86 'Microsoft Visual Studio\Installer'
    $vsWherePath = Join-Path $installerRoot 'vswhere.exe'
    $installerPath = Join-Path $installerRoot 'setup.exe'
    if (-not (Test-Path -LiteralPath $vsWherePath -PathType Leaf)) {
        return [pscustomobject]@{
            InstallationPath = $null
            InstanceResolved = $false
            WorkloadInstalled = $false
            InstallerPath = $installerPath
            InstallerAvailable = (Test-Path -LiteralPath $installerPath -PathType Leaf)
        }
    }

    try {
        $installationPath = Invoke-DashyVsWhereInstallationPath -VsWherePath $vsWherePath
        $workloadPath = Invoke-DashyVsWhereInstallationPath -VsWherePath $vsWherePath -RequiresVCTools
    }
    catch {
        return [pscustomobject]@{
            InstallationPath = $null
            InstanceResolved = $false
            WorkloadInstalled = $false
            InstallerPath = $installerPath
            InstallerAvailable = (Test-Path -LiteralPath $installerPath -PathType Leaf)
        }
    }

    $instanceResolved = -not [string]::IsNullOrWhiteSpace($installationPath) -and (Test-Path -LiteralPath $installationPath -PathType Container)
    $workloadInstalled = -not [string]::IsNullOrWhiteSpace($workloadPath) -and (Test-Path -LiteralPath $workloadPath -PathType Container)
    if ($workloadInstalled -and -not $instanceResolved) {
        $installationPath = $workloadPath
        $instanceResolved = $true
    }

    return [pscustomobject]@{
        InstallationPath = $installationPath
        InstanceResolved = $instanceResolved
        WorkloadInstalled = $workloadInstalled
        InstallerPath = $installerPath
        InstallerAvailable = (Test-Path -LiteralPath $installerPath -PathType Leaf)
    }
}

function Install-DashyVisualStudioVCToolsWorkload {
    param(
        [Parameter(Mandatory)][string]$InstallerPath,
        [Parameter(Mandatory)][string]$InstallationPath
    )

    Write-Host 'Adding the Visual Studio C++ Build Tools workload to the existing instance. The official Visual Studio Installer may request elevation.'
    & $InstallerPath modify --installPath $InstallationPath --add $dashyVisualStudioVCToolsWorkloadId --includeRecommended --passive --norestart
    if ($LASTEXITCODE -ne 0) {
        throw "Visual Studio Installer could not add $dashyVisualStudioVCToolsWorkloadId (exit code $LASTEXITCODE)."
    }
}

function Stop-DashyVisualStudioRepair {
    throw "Visual Studio Build Tools is installed without $dashyVisualStudioVCToolsWorkloadId, but Dashy could not safely resolve both the existing instance and the official Visual Studio Installer. Open Visual Studio Installer, choose Modify for Build Tools 2022, select Desktop development with C++ (workload ID: $dashyVisualStudioVCToolsWorkloadId), include the recommended components, apply the change, and rerun install.ps1."
}

function Get-DashyTauriCliVersionState {
    param(
        [Parameter(Mandatory)][bool]$CommandVisible,
        [Parameter(Mandatory)][int]$ExitCode,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Output
    )

    if (-not $CommandVisible) {
        return 'Missing'
    }
    if ($ExitCode -ne 0) {
        return 'Unusable'
    }

    $versionMatch = [regex]::Match($Output, '(?im)^\s*(?:tauri-cli|cargo-tauri)\s+([0-9]+)(?:\.|$)')
    if (-not $versionMatch.Success) {
        return 'Unrecognized'
    }
    if ([int]$versionMatch.Groups[1].Value -eq 2) {
        return 'CompatibleV2'
    }

    return 'IncompatibleMajor'
}

function Get-DashyTauriCliState {
    $commandVisible = Test-DashyCommand -Name 'cargo-tauri'
    if (-not $commandVisible) {
        return 'Missing'
    }

    $output = & cargo-tauri --version 2>&1 | Out-String
    return Get-DashyTauriCliVersionState -CommandVisible $true -ExitCode $LASTEXITCODE -Output $output
}

function Install-DashyWingetPackage {
    param([Parameter(Mandatory)]$Spec)

    $arguments = @(
        'install',
        '--id', $Spec.Id,
        '--exact',
        '--accept-package-agreements',
        '--accept-source-agreements'
    )

    if ($Spec.Override) {
        Write-Host "Installing $($Spec.Id). The official Visual Studio installer may request elevation; approve only its own prompt if you want to continue."
        $arguments += @('--override', $Spec.Override)
    }
    else {
        Write-Host "Installing $($Spec.Id)..."
    }

    & winget @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "winget could not install $($Spec.Id) (exit code $LASTEXITCODE)."
    }
}

function Stop-DashyUntilNewShell {
    param([Parameter(Mandatory)][string[]]$MissingCommands)

    Write-Host ''
    Write-Host 'The following required commands are still not visible in this PowerShell session:' -ForegroundColor Yellow
    $MissingCommands | ForEach-Object { Write-Host "  - $_" -ForegroundColor Yellow }
    Write-Host 'Open a new PowerShell window and run this script again. No project dependencies were installed.' -ForegroundColor Yellow
    exit 1
}

function Stop-DashyInstalledPackageRepair {
    param(
        [Parameter(Mandatory)]$Spec,
        [Parameter(Mandatory)][string[]]$MissingCommands
    )

    Write-Host ''
    Write-Host "$($Spec.Id) is installed, but its required commands are not all available:" -ForegroundColor Yellow
    $MissingCommands | ForEach-Object { Write-Host "  - $_" -ForegroundColor Yellow }
    Write-Host 'Open a new PowerShell window once. If the commands are still unavailable, repair the installed package with its official installer or Windows Apps settings, then rerun this script. Dashy will not reinstall, downgrade, or remove the package.' -ForegroundColor Yellow
    exit 1
}

if ($CheckOnly) {
    $checks = @(
        $requiredCommands |
            ForEach-Object {
                $installed = if ($_ -eq 'cargo-tauri') {
                    (Get-DashyTauriCliState) -eq 'CompatibleV2'
                }
                else {
                    Test-DashyCommand -Name $_
                }

                [pscustomobject]@{
                    Command = $_
                    Installed = $installed
                }
            }
    )
    $buildToolsProbe = Get-DashyBuildToolsProbe
    $checks += [pscustomobject]@{
        Command = $dashyVisualStudioVCToolsWorkloadId
        Installed = $buildToolsProbe.WorkloadInstalled
    }
    $checks |
        ForEach-Object {
            [pscustomobject]@{
                Command = $_.Command
                Installed = $_.Installed
            }
        } |
        Format-Table -AutoSize
    return
}

Update-DashyProcessPath

if (-not (Test-DashyCommand -Name 'winget')) {
    throw 'WinGet is required to install Dashy prerequisites. Install or update Microsoft App Installer, open a new PowerShell window, and run this script again.'
}

Write-Host 'Some official installers may request elevation. CheckOnly never requests elevation.'

foreach ($spec in $packageSpecs) {
    if ($spec.Id -eq $dashyVisualStudioBuildToolsId) {
        $packageInstalled = Test-DashyWingetPackage -Id $spec.Id
        $probe = Get-DashyBuildToolsProbe
        $buildToolsInstalled = $packageInstalled -or $probe.InstanceResolved
        $action = Get-DashyBuildToolsAction -PackageInstalled $buildToolsInstalled -WorkloadInstalled $probe.WorkloadInstalled -InstanceResolved $probe.InstanceResolved -InstallerAvailable $probe.InstallerAvailable

        if ($action -eq 'InstallPackage') {
            Install-DashyWingetPackage -Spec $spec
            $packageInstalled = $true
            $probe = Get-DashyBuildToolsProbe
            $action = Get-DashyBuildToolsAction -PackageInstalled $packageInstalled -WorkloadInstalled $probe.WorkloadInstalled -InstanceResolved $probe.InstanceResolved -InstallerAvailable $probe.InstallerAvailable
        }

        if ($action -eq 'SkipAvailable') {
            Write-Host "$($spec.Id) with $dashyVisualStudioVCToolsWorkloadId is already installed; skipping."
            continue
        }
        if ($action -eq 'ModifyExisting') {
            Install-DashyVisualStudioVCToolsWorkload -InstallerPath $probe.InstallerPath -InstallationPath $probe.InstallationPath
            $verifiedProbe = Get-DashyBuildToolsProbe
            if (-not $verifiedProbe.WorkloadInstalled) {
                Stop-DashyVisualStudioRepair
            }
            continue
        }

        Stop-DashyVisualStudioRepair
    }

    $visibleCommands = @($spec.Commands | Where-Object { Test-DashyCommand -Name $_ })
    if ($spec.Commands.Count -gt 0 -and $visibleCommands.Count -eq $spec.Commands.Count) {
        Write-Host "$($spec.Id) is already available through '$($spec.Commands -join ', ')'; skipping."
        continue
    }

    $packageInstalled = Test-DashyWingetPackage -Id $spec.Id
    $decision = Get-DashyPackageDecision -RequiredCommands $spec.Commands -VisibleCommands $visibleCommands -PackageInstalled $packageInstalled
    if ($decision -eq 'SkipInstalled') {
        Write-Host "$($spec.Id) is already installed; skipping."
        continue
    }

    if ($decision -eq 'StopPartialInstall') {
        $missingCommands = @($spec.Commands | Where-Object { $_ -notin $visibleCommands })
        Stop-DashyInstalledPackageRepair -Spec $spec -MissingCommands $missingCommands
    }

    Install-DashyWingetPackage -Spec $spec
}

Update-DashyProcessPath
$missingPrerequisites = @($requiredCommands | Where-Object { $_ -ne 'cargo-tauri' -and -not (Test-DashyCommand -Name $_) })
if ($missingPrerequisites.Count -gt 0) {
    Stop-DashyUntilNewShell -MissingCommands $missingPrerequisites
}

$tauriCliState = Get-DashyTauriCliState
if ($tauriCliState -eq 'Missing') {
    Write-Host 'Installing Tauri CLI v2...'
    & cargo install tauri-cli --version '^2.0.0' --locked
    if ($LASTEXITCODE -ne 0) {
        throw "cargo could not install tauri-cli (exit code $LASTEXITCODE)."
    }
}
elseif ($tauriCliState -eq 'CompatibleV2') {
    Write-Host 'Tauri CLI is already available; skipping.'
}
else {
    throw "cargo-tauri is installed but was not verified as tauri-cli major version 2 (state: $tauriCliState). Run cargo install tauri-cli --version '^2.0.0' --locked --force, confirm cargo-tauri --version reports major version 2, and rerun install.ps1."
}

Update-DashyProcessPath
$missingCommands = @($requiredCommands | Where-Object { $_ -ne 'cargo-tauri' -and -not (Test-DashyCommand -Name $_) })
if ($missingCommands.Count -gt 0) {
    Stop-DashyUntilNewShell -MissingCommands $missingCommands
}
$tauriCliState = Get-DashyTauriCliState
if ($tauriCliState -eq 'Missing') {
    Stop-DashyUntilNewShell -MissingCommands @('cargo-tauri')
}
if ($tauriCliState -ne 'CompatibleV2') {
    throw "cargo-tauri was not verified as tauri-cli major version 2 after setup (state: $tauriCliState). Run cargo install tauri-cli --version '^2.0.0' --locked --force, confirm cargo-tauri --version reports major version 2, and rerun install.ps1."
}

$frontendPath = Join-Path $PSScriptRoot 'frontend'
if (-not (Test-Path -LiteralPath $frontendPath -PathType Container)) {
    throw "The frontend directory was not found at '$frontendPath'."
}

Write-Host 'Installing the locked frontend dependencies...'
& npm ci --prefix $frontendPath
if ($LASTEXITCODE -ne 0) {
    throw "npm ci failed (exit code $LASTEXITCODE)."
}

Write-Host 'Prerequisites and project dependencies are ready. Sign in to providers manually using the README instructions before starting Dashy.'
