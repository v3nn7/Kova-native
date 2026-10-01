param(
    [switch]$Plan,
    [ValidateRange(1, 1440)][int]$MaxMinutes = 65,
    [DateTimeOffset]$NotBeforeUtc = [DateTimeOffset]::MinValue
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$workspace = Split-Path -Parent $PSScriptRoot
$headers = @{ 'User-Agent' = 'Kova-Native-release (https://github.com/v3nn7/Kova-native)' }
$deadline = [DateTimeOffset]::UtcNow.AddMinutes($MaxMinutes)
Push-Location -LiteralPath $workspace
try {
    $releaseCommit = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot determine release commit' }
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed' }
    $remaining = @($metadata.packages | Where-Object {
        $_.id -in $metadata.workspace_members -and $_.publish -contains 'crates-io'
    })
    $ordered = @()
    while ($remaining.Count -gt 0) {
        $next = $remaining | Sort-Object name | Where-Object {
            @($_.dependencies | Where-Object {
                $_.kind -ne 'dev' -and $_.name -in $remaining.name
            }).Count -eq 0
        } | Select-Object -First 1
        if ($null -eq $next) { throw 'Cycle in publishable workspace dependencies' }
        $ordered += $next
        $remaining = @($remaining | Where-Object { $_.id -ne $next.id })
    }

    foreach ($package in $ordered) {
        $exists = $false
        try {
            $published = Invoke-RestMethod -Uri "https://crates.io/api/v1/crates/$($package.name)/$($package.version)" -Headers $headers
            $exists = $published.version.num -eq $package.version
        } catch {
            if ([int]$_.Exception.Response.StatusCode -ne 404) { throw }
        }
        if ($exists) {
            Write-Output "Already published: $($package.name) $($package.version)"
            continue
        }
        if ($Plan) {
            Write-Output "Would publish: $($package.name) $($package.version)"
            continue
        }

        while ($true) {
            if ([DateTimeOffset]::UtcNow -ge $deadline) { throw 'Publication time limit exceeded; rerun to resume' }
            while ([DateTimeOffset]::UtcNow -lt $NotBeforeUtc) {
                if ([DateTimeOffset]::UtcNow -ge $deadline) { throw 'Publication time limit exceeded; rerun to resume' }
                Write-Output "Waiting for registry allowance: $($package.name); next attempt $($NotBeforeUtc.ToUniversalTime().ToString('u'))"
                $seconds = [Math]::Min(30, [Math]::Ceiling(($NotBeforeUtc - [DateTimeOffset]::UtcNow).TotalSeconds))
                Start-Sleep -Seconds ([Math]::Max(1, $seconds))
            }
            $currentCommit = git rev-parse HEAD
            $changes = @(git status --porcelain)
            if ($LASTEXITCODE -ne 0 -or $currentCommit -ne $releaseCommit -or $changes.Count -ne 0) {
                throw 'Release checkout changed; review and commit changes before resuming'
            }
            Write-Output "Publishing $($package.name) $($package.version)"
            $output = @(& cargo publish -p $package.name 2>&1)
            $publishExit = $LASTEXITCODE
            $output | ForEach-Object { Write-Output $_.ToString() }
            if ($publishExit -eq 0) { break }
            $message = ($output | ForEach-Object { $_.ToString() }) -join "`n"
            if ($message -match 'status 429' -and $message -match 'Please try again after (.+? GMT)') {
                $NotBeforeUtc = [DateTimeOffset]::ParseExact(
                    $Matches[1], "ddd, dd MMM yyyy HH:mm:ss 'GMT'",
                    [Globalization.CultureInfo]::InvariantCulture,
                    [Globalization.DateTimeStyles]::AssumeUniversal
                ).AddSeconds(2)
                continue
            }
            throw "Publication failed for $($package.name) (exit $publishExit); rerun after resolving the error"
        }
    }
    if (-not $Plan) { Write-Output 'All publishable workspace versions are available on crates.io.' }
} finally {
    Pop-Location
}
