$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $workspace
try {
    cargo build --workspace --bins
    if ($LASTEXITCODE -ne 0) { throw 'Example build failed' }
    $logDirectory = Join-Path $workspace 'target/native-smoke'
    New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
    foreach ($example in @('hello_window', 'buttons', 'layout', 'animation', 'showcase', 'showcase-motion', 'showcase-typography')) {
        Write-Host "Running native smoke: $example"
        $binary = if ($example.StartsWith('showcase-')) { 'showcase' } else { $example }
        $arguments = @('--smoke')
        if ($example -eq 'showcase-motion') { $arguments += @('--page', 'motion') }
        if ($example -eq 'showcase-typography') { $arguments += @('--page', 'typography', '--light') }
        $executable = Join-Path $workspace "target/debug/$binary.exe"
        $stdout = Join-Path $logDirectory "$example.stdout.log"
        $stderr = Join-Path $logDirectory "$example.stderr.log"
        $process = Start-Process -FilePath $executable -ArgumentList $arguments -WindowStyle Hidden -PassThru `
            -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        if (-not $process.WaitForExit(30000)) {
            Stop-Process -Id $process.Id -ErrorAction SilentlyContinue
            throw "Native startup timed out: $example. Logs: $logDirectory"
        }
        $process.Refresh()
        Get-Content -LiteralPath $stdout
        if ($process.ExitCode -ne 0) {
            Get-Content -LiteralPath $stderr
            throw "Native smoke failed: $example (exit $($process.ExitCode))"
        }
    }
} finally {
    Pop-Location
}
