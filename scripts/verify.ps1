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
        [Parameter(ValueFromRemainingArguments = $true)]
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

Invoke-Checked "cargo fmt --all -- --check" cargo fmt --all -- --check
Invoke-Checked "cargo test --workspace" cargo test --workspace
Invoke-Checked "cargo clippy --workspace --all-targets -- -D warnings" cargo clippy --workspace --all-targets -- -D warnings

Write-Host ""
Write-Host "Strict compat: set GIT_SVN_RS_STRICT_COMPAT=1 or pass -StrictCompat."
Write-Host "When strict compat is enabled, tests that need svnadmin/svn fail if those tools are missing."
