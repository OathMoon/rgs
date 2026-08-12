param(
    [switch]$StrictCompat
)

$ErrorActionPreference = "Stop"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    Write-Host "Running $Label"
    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

if ($StrictCompat) {
    $env:GIT_SVN_RS_STRICT_COMPAT = "1"
    if (-not $env:GIT_SVN_RS_COMPAT_ARTIFACT_DIR) {
        $env:GIT_SVN_RS_COMPAT_ARTIFACT_DIR = Join-Path ([System.IO.Path]::GetTempPath()) "git-svn-rs-compat-$PID"
    }
    New-Item -ItemType Directory -Force -Path $env:GIT_SVN_RS_COMPAT_ARTIFACT_DIR | Out-Null
}

Invoke-Checked "cargo fmt --all -- --check" "cargo" @("fmt", "--all", "--", "--check")
Invoke-Checked "cargo test --workspace" "cargo" @("test", "--workspace")
if ($StrictCompat) {
    $requiredScenarios = @(
        "standard-linear-history",
        "standard-layout-history",
        "standard-full-url-layout-history",
        "standard-subdirectory-history",
        "standard-dcommit-write",
        "authenticated-svn-dcommit-write",
        "recovered-dcommit-write",
        "dirty-dcommit-no-write"
    )
    foreach ($scenario in $requiredScenarios) {
        $summaryPath = Join-Path $env:GIT_SVN_RS_COMPAT_ARTIFACT_DIR "$scenario/scenario-summary.json"
        if (-not (Test-Path $summaryPath)) {
            throw "Required compatibility summary is missing: $summaryPath"
        }
        $summary = Get-Content -Raw $summaryPath | ConvertFrom-Json
        if ($summary.status -ne "passed" -or $summary.execution -ne "executed") {
            throw "Required compatibility scenario did not execute and pass: $scenario"
        }
        if ($summary.frozen_git_commit -ne "0b13e48a3a30cdfa94e8ef842e24d6045ab3d015") {
            throw "Required compatibility scenario used the wrong frozen Git commit: $scenario"
        }
        if ($summary.comparison_backend -ne "svn-cli-vs-frozen-perl" -or $summary.build_features -ne "default-svn-cli") {
            throw "Required compatibility scenario has the wrong backend/feature profile: $scenario"
        }
    }
    Invoke-Checked "linked libsvn integration" "cargo" @("test", "-p", "git-svn-rs-core", "--features", "svn-libsvn")
    Invoke-Checked "linked libsvn serial diagnostic" "cargo" @("test", "-p", "git-svn-rs-core", "--features", "svn-libsvn", "--", "--test-threads=1")
    Invoke-Checked "linked libsvn CLI workflows" "cargo" @("test", "-p", "git-svn-rs", "--features", "svn-libsvn", "--test", "clone_fetch_real_svn")
    Invoke-Checked "linked libsvn dcommit workflows" "cargo" @("test", "-p", "git-svn-rs", "--features", "svn-libsvn", "--test", "dcommit_linear")
}
Invoke-Checked "cargo clippy --all-targets --all-features -- -D warnings" "cargo" @("clippy", "--all-targets", "--all-features", "--", "-D", "warnings")

if ($StrictCompat) {
    $commitSha = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) {
        throw "git rev-parse HEAD failed with exit code $LASTEXITCODE"
    }
    $releaseSummary = [ordered]@{
        schema_version = 2
        status = "passed"
        commit_sha = $commitSha
        required_scenario_count = 8
        scenario_backend = "svn-cli-vs-frozen-perl"
        scenario_build_features = "default-svn-cli"
        linked_backend_profiles = @("core-parallel", "core-serial", "cli-read-import", "cli-dcommit-post-fetch")
    }
    $releaseSummary | ConvertTo-Json -Depth 3 | Set-Content -Encoding utf8 (Join-Path $env:GIT_SVN_RS_COMPAT_ARTIFACT_DIR "release-summary.json")
    Write-Host "Compatibility artifacts: $env:GIT_SVN_RS_COMPAT_ARTIFACT_DIR"
}

Write-Host ""
Write-Host "Strict compat: set GIT_SVN_RS_STRICT_COMPAT=1 or pass -StrictCompat."
Write-Host "When strict compat is enabled, tests that need svnadmin/svn or Perl git-svn fail if those tools are missing."
