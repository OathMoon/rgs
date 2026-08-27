param(
    [switch]$AllowDirty,
    [switch]$KeepTemp
)

$ErrorActionPreference = "Stop"
$workspaceRoot = Split-Path -Parent $PSScriptRoot
$version = "0.1.0"
$packages = @(
    [ordered]@{ Name = "git-svn-rs-core"; Version = $version },
    [ordered]@{ Name = "git-svn-rs"; Version = $version },
    [ordered]@{ Name = "git-svn-rs-shim"; Version = $version }
)
$forbiddenPathPattern = '(^|[\\/])(golden-stdlayout-|svn-fixture-|\.svn([\\/]|$)|\.git([\\/]|$)|\.plans([\\/]|$)|\.github([\\/]|$)|\.codex([\\/]|$)|\.zcode([\\/]|$))'
$requiredFiles = @("README.md", "LICENSE-MIT", "LICENSE-APACHE")
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "git-svn-rs-package-$PID-$([Guid]::NewGuid().ToString('N'))"
$indexRoot = Join-Path $tempRoot "index"
$downloadRoot = Join-Path $tempRoot "crates"
$extractRoot = Join-Path $tempRoot "extract"
$packageTargetRoot = Join-Path $tempRoot "target"
$cleanCargoHome = Join-Path $tempRoot "cargo-home"
$isolatedWorkspace = Join-Path $tempRoot "workspace"
$workspaceMetadata = $null

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [string]$WorkingDirectory = $workspaceRoot
    )

    Write-Host "Running $Label"
    Push-Location $WorkingDirectory
    try {
        & $FilePath @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$Label failed with exit code $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
}

function Get-IndexRelativePath {
    param([Parameter(Mandatory = $true)][string]$Name)

    $lower = $Name.ToLowerInvariant()
    switch ($lower.Length) {
        1 { return "1/$lower" }
        2 { return "2/$lower" }
        3 { return "3/$($lower.Substring(0, 1))/$lower" }
        default { return "$($lower.Substring(0, 2))/$($lower.Substring(2, 2))/$lower" }
    }
}

function Get-RegistryEntry {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$Checksum
    )

    $package = $workspaceMetadata.packages | Where-Object { $_.name -eq $Name -and $_.version -eq $Version }
    if (-not $package) {
        throw "workspace metadata does not contain $Name $Version"
    }

    $dependencies = @($package.dependencies | ForEach-Object {
        $kind = if ($_.kind) { $_.kind } else { "normal" }
        $dependency = [ordered]@{
            name = if ($_.rename) { $_.rename } else { $_.name }
            req = $_.req
            features = @($_.features)
            optional = [bool]$_.optional
            default_features = [bool]$_.uses_default_features
            target = $_.target
            kind = $kind
            registry = $null
        }
        if ($_.rename) {
            $dependency["package"] = $_.name
        }
        $dependency
    })

    $features = [ordered]@{}
    foreach ($property in $package.features.PSObject.Properties) {
        $features[$property.Name] = @($property.Value)
    }

    return [ordered]@{
        name = $Name
        vers = $Version
        deps = $dependencies
        cksum = $Checksum
        features = $features
        yanked = $false
        links = $package.links
        rust_version = $package.rust_version
        v = 2
    }
}

function Add-RegistryPackage {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$CratePath,
        [switch]$Quiet
    )

    $checksum = (Get-FileHash -Algorithm SHA256 -LiteralPath $CratePath).Hash.ToLowerInvariant()
    Copy-Item -LiteralPath $CratePath -Destination (Join-Path $downloadRoot "$Name-$Version.crate") -Force
    $relativeIndexPath = Get-IndexRelativePath -Name $Name
    $indexPath = Join-Path $indexRoot $relativeIndexPath
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $indexPath) | Out-Null
    $entry = Get-RegistryEntry -Name $Name -Version $Version -Checksum $checksum
    $entryLine = ($entry | ConvertTo-Json -Compress -Depth 8) + "`n"
    if (Test-Path -LiteralPath $indexPath) {
        [System.IO.File]::AppendAllText($indexPath, $entryLine, [System.Text.UTF8Encoding]::new($false))
    } else {
        [System.IO.File]::WriteAllText($indexPath, $entryLine, [System.Text.UTF8Encoding]::new($false))
    }
    if (-not $Quiet) {
        Write-Host "$Name $Version sha256 $checksum"
    }
}

function Commit-RegistryIndex {
    param([Parameter(Mandatory = $true)][string]$Message)

    Invoke-Checked "stage temporary registry index" "git" @("add", ".") $indexRoot
    Invoke-Checked "commit temporary registry index" "git" @("-c", "user.name=git-svn-rs release audit", "-c", "user.email=release-audit@example.invalid", "commit", "--quiet", "-m", $Message) $indexRoot
}

function Add-CachedRegistryPackages {
    $cargoHomePath = if ($env:CARGO_HOME) {
        $env:CARGO_HOME
    } else {
        Join-Path $env:USERPROFILE ".cargo"
    }
    $cacheRoot = Join-Path $cargoHomePath "registry/cache"
    if (-not (Test-Path -LiteralPath $cacheRoot)) {
        throw "Cargo registry cache not found at $cacheRoot; run cargo fetch first"
    }

    $cachedCrates = @{}
    Get-ChildItem -LiteralPath $cacheRoot -Recurse -File -Filter "*.crate" | ForEach-Object {
        $cachedCrates[$_.Name] = $_.FullName
    }
    $registryPackages = @($workspaceMetadata.packages | Where-Object { $_.source -like "registry+*" })
    $requestedFeatures = @{}
    foreach ($metadataPackage in $workspaceMetadata.packages) {
        foreach ($dependency in $metadataPackage.dependencies) {
            if (-not $requestedFeatures.ContainsKey($dependency.name)) {
                $requestedFeatures[$dependency.name] = @{}
            }
            foreach ($feature in @($dependency.features)) {
                $requestedFeatures[$dependency.name][$feature] = $true
            }
            if ($dependency.uses_default_features) {
                $requestedFeatures[$dependency.name]["default"] = $true
            }
        }
    }
    $metadataKeys = @{}
    foreach ($package in $registryPackages) {
        $metadataKeys[$package.name + "@" + $package.version] = $true
        $fileName = "$($package.name)-$($package.version).crate"
        $cratePath = $cachedCrates[$fileName]
        if (-not $cratePath) {
            throw "cached crate not found for $($package.name) $($package.version); run cargo fetch first"
        }
        Add-RegistryPackage -Name $package.name -Version $package.version -CratePath $cratePath -Quiet
    }

    $placeholderCount = 0
    $lockBlocks = (Get-Content -Raw (Join-Path $workspaceRoot "Cargo.lock")) -split '\r?\n\[\[package\]\]\r?\n'
    foreach ($block in $lockBlocks) {
        $name = [regex]::Match($block, '(?m)^name = "([^"]+)"').Groups[1].Value
        $version = [regex]::Match($block, '(?m)^version = "([^"]+)"').Groups[1].Value
        $source = [regex]::Match($block, '(?m)^source = "([^"]+)"').Groups[1].Value
        $checksum = [regex]::Match($block, '(?m)^checksum = "([^"]+)"').Groups[1].Value
        if (-not $name -or -not $version -or -not $source.StartsWith("registry+") -or $metadataKeys.ContainsKey($name + "@" + $version)) {
            continue
        }

        $features = [ordered]@{}
        if ($requestedFeatures[$name]) {
            foreach ($feature in $requestedFeatures[$name].Keys) {
                $features[$feature] = @()
            }
        }
        $entry = [ordered]@{
            name = $name
            vers = $version
            deps = @()
            cksum = $checksum
            features = $features
            yanked = $false
        }
        $relativeIndexPath = Get-IndexRelativePath -Name $name
        $indexPath = Join-Path $indexRoot $relativeIndexPath
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $indexPath) | Out-Null
        $entryLine = ($entry | ConvertTo-Json -Compress -Depth 4) + "`n"
        if (Test-Path -LiteralPath $indexPath) {
            [System.IO.File]::AppendAllText($indexPath, $entryLine, [System.Text.UTF8Encoding]::new($false))
        } else {
            [System.IO.File]::WriteAllText($indexPath, $entryLine, [System.Text.UTF8Encoding]::new($false))
        }
        $fileName = "$name-$version.crate"
        if ($cachedCrates[$fileName]) {
            Copy-Item -LiteralPath $cachedCrates[$fileName] -Destination (Join-Path $downloadRoot $fileName) -Force
        }
        $placeholderCount += 1
    }
    Commit-RegistryIndex -Message "Mirror locked registry dependencies"
    Write-Host "Temporary registry mirrored $($registryPackages.Count) host packages and indexed $placeholderCount inactive locked packages"
}

function Expand-Package {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Version,
        [Parameter(Mandatory = $true)][string]$CratePath
    )

    $destination = Join-Path $extractRoot $Name
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Invoke-Checked "extract $Name package" "tar" @("-xzf", $CratePath, "-C", $destination)
    return Join-Path $destination "$Name-$Version"
}

function Copy-PackageWorkspace {
    New-Item -ItemType Directory -Force -Path $isolatedWorkspace, (Join-Path $isolatedWorkspace "crates") | Out-Null
    foreach ($file in @("Cargo.toml", "Cargo.lock", "README.md")) {
        Copy-Item -LiteralPath (Join-Path $workspaceRoot $file) -Destination (Join-Path $isolatedWorkspace $file)
    }

    $packageFiles = [ordered]@{
        "git-svn-rs-core" = @("Cargo.toml", "build.rs", "LICENSE-APACHE", "LICENSE-MIT", "src", "tests")
        "git-svn-rs-cli" = @("Cargo.toml", "LICENSE-APACHE", "LICENSE-MIT", "src", "tests")
        "git-svn-rs-shim" = @("Cargo.toml", "LICENSE-APACHE", "LICENSE-MIT", "src")
    }
    foreach ($packageName in $packageFiles.Keys) {
        $source = Join-Path $workspaceRoot "crates/$packageName"
        $destination = Join-Path $isolatedWorkspace "crates/$packageName"
        New-Item -ItemType Directory -Force -Path $destination | Out-Null
        foreach ($file in $packageFiles[$packageName]) {
            Copy-Item -LiteralPath (Join-Path $source $file) -Destination $destination -Recurse
        }
    }
}

New-Item -ItemType Directory -Force -Path $indexRoot, $downloadRoot, $extractRoot, $packageTargetRoot, $cleanCargoHome | Out-Null
try {
    $hostTriple = ((rustc -vV | Select-String '^host: ').Line -replace '^host: ', '')
    if (-not $hostTriple) {
        throw "rustc did not report a host target"
    }
    $workspaceMetadata = (& cargo metadata --offline --filter-platform $hostTriple --format-version 1 | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed with exit code $LASTEXITCODE"
    }
    $downloadUri = [System.Uri]::new((Resolve-Path $downloadRoot).Path).AbsoluteUri.TrimEnd('/')
    $registryConfig = [ordered]@{ dl = "$downloadUri/{crate}-{version}.crate" } | ConvertTo-Json -Compress
    [System.IO.File]::WriteAllText(
        (Join-Path $indexRoot "config.json"),
        $registryConfig,
        [System.Text.UTF8Encoding]::new($false)
    )
    Invoke-Checked "initialize temporary registry" "git" @("init", "--quiet") $indexRoot
    Commit-RegistryIndex -Message "Initialize registry"
    Add-CachedRegistryPackages
    Copy-PackageWorkspace

    $packageFlag = if ($AllowDirty) { @("--allow-dirty") } else { @() }
    $indexUri = [System.Uri]::new((Resolve-Path $indexRoot).Path).AbsoluteUri
    $sourceConfigArgs = @(
        "--config", "source.crates-io.replace-with='phase10'",
        "--config", "source.phase10.registry='$indexUri'"
    )
    $previousCargoHome = $env:CARGO_HOME
    $env:CARGO_HOME = $cleanCargoHome
    try {
        foreach ($package in $packages) {
            $listArgs = @("package", "-p", $package.Name, "--index", $indexUri) + $packageFlag + $sourceConfigArgs + @("--list")
            Push-Location $isolatedWorkspace
            try {
                $packageList = @(& cargo @listArgs)
            } finally {
                Pop-Location
            }
            if ($LASTEXITCODE -ne 0) {
                throw "cargo package --list failed for $($package.Name)"
            }
            foreach ($required in $requiredFiles) {
                if ($packageList -notcontains $required) {
                    throw "$($package.Name) package is missing $required"
                }
            }
            $forbidden = @($packageList | Where-Object { $_ -match $forbiddenPathPattern })
            if ($forbidden.Count -gt 0) {
                throw "$($package.Name) package contains forbidden paths: $($forbidden -join ', ')"
            }
            Write-Host "$($package.Name) package list audited ($($packageList.Count) files)"

            $packageArgs = @(
                "package", "-p", $package.Name, "--index", $indexUri,
                "--target-dir", $packageTargetRoot, "--exclude-lockfile"
            ) + $packageFlag + $sourceConfigArgs
            Invoke-Checked "package $($package.Name)" "cargo" $packageArgs $isolatedWorkspace
            $cratePath = Join-Path $packageTargetRoot "package/$($package.Name)-$($package.Version).crate"
            if (-not (Test-Path -LiteralPath $cratePath)) {
                throw "cargo did not create $cratePath"
            }

            $expanded = Expand-Package -Name $package.Name -Version $package.Version -CratePath $cratePath
            $manifest = Join-Path $expanded "Cargo.toml"
            $checkArgs = @("check", "--manifest-path", $manifest) + $sourceConfigArgs
            Invoke-Checked "isolated check for $($package.Name)" "cargo" $checkArgs $expanded

            Add-RegistryPackage -Name $package.Name -Version $package.Version -CratePath $cratePath
            Commit-RegistryIndex -Message "Add $($package.Name) $($package.Version)"
        }
    } finally {
        $env:CARGO_HOME = $previousCargoHome
    }

    Write-Host "Publish order verified: git-svn-rs-core -> git-svn-rs -> git-svn-rs-shim"
} finally {
    if ($KeepTemp) {
        Write-Host "Package verification files retained at $tempRoot"
    } elseif (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
