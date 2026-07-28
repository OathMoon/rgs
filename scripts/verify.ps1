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
}

Invoke-Checked "cargo fmt --all -- --check" "cargo" @("fmt", "--all", "--", "--check")
Invoke-Checked "cargo test --workspace" "cargo" @("test", "--workspace")
if ($StrictCompat) {
    Invoke-Checked "linked libsvn integration" "cargo" @("test", "-p", "git-svn-rs-core", "--features", "svn-libsvn")
    Invoke-Checked "linked libsvn serial diagnostic" "cargo" @("test", "-p", "git-svn-rs-core", "--features", "svn-libsvn", "--", "--test-threads=1")
    Invoke-Checked "linked libsvn CLI workflows" "cargo" @("test", "-p", "git-svn-rs", "--features", "svn-libsvn", "--test", "clone_fetch_real_svn")
}
Invoke-Checked "cargo clippy --all-targets --all-features -- -D warnings" "cargo" @("clippy", "--all-targets", "--all-features", "--", "-D", "warnings")

Write-Host ""
Write-Host "Strict compat: set GIT_SVN_RS_STRICT_COMPAT=1 or pass -StrictCompat."
Write-Host "When strict compat is enabled, tests that need svnadmin/svn or Perl git-svn fail if those tools are missing."
