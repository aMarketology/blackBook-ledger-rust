# Alice & Bob Integration Tests Runner
# Runs all integration tests using the exposed test account keys
#
# Usage:
#   .\run_alice_bob_tests.ps1           # Run all tests
#   .\run_alice_bob_tests.ps1 -Filter "alice"  # Run tests containing "alice"
#   .\run_alice_bob_tests.ps1 -Verbose  # Run with verbose output

param(
    [string]$Filter = "",
    [switch]$Verbose,
    [switch]$NoCaptured,
    [switch]$StartServer
)

Write-Host ""
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "     🧪 Alice & Bob Integration Tests" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

Write-Host "📋 Test Accounts:" -ForegroundColor Yellow
Write-Host "   👩 Alice: L1ALICE000000001 (10,000 BB)" -ForegroundColor White
Write-Host "   👨 Bob:   L1BOB0000000001  (5,000 BB)" -ForegroundColor White
Write-Host ""

# Check if server is running
Write-Host "🔍 Checking server status..." -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "http://localhost:1234/health" -TimeoutSec 2 -ErrorAction Stop
    Write-Host "✅ Server is running at http://localhost:1234" -ForegroundColor Green
} catch {
    Write-Host "⚠️  Server not running at http://localhost:1234" -ForegroundColor Red
    
    if ($StartServer) {
        Write-Host "🚀 Starting server..." -ForegroundColor Yellow
        Start-Process -FilePath "cargo" -ArgumentList "run" -NoNewWindow
        Start-Sleep -Seconds 3
        Write-Host "✅ Server started" -ForegroundColor Green
    } else {
        Write-Host ""
        Write-Host "To start the server, run one of:" -ForegroundColor Yellow
        Write-Host "   cargo run" -ForegroundColor White
        Write-Host "   .\run_alice_bob_tests.ps1 -StartServer" -ForegroundColor White
        Write-Host ""
        exit 1
    }
}

Write-Host ""
Write-Host "🧪 Running integration tests..." -ForegroundColor Yellow
Write-Host ""

# Build cargo test command
$testCmd = "cargo test"

if ($Filter) {
    $testCmd += " $Filter"
    Write-Host "   Filter: $Filter" -ForegroundColor Gray
}

$testCmd += " --test alice_bob_integration_test"

if ($NoCaptured) {
    $testCmd += " -- --nocapture"
}

if ($Verbose) {
    $testCmd += " -- --nocapture"
    Write-Host "   Mode: Verbose" -ForegroundColor Gray
}

Write-Host "   Command: $testCmd" -ForegroundColor Gray
Write-Host ""

# Run the tests
Invoke-Expression $testCmd

$exitCode = $LASTEXITCODE

Write-Host ""
if ($exitCode -eq 0) {
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Green
    Write-Host "     ✅ ALL TESTS PASSED" -ForegroundColor Green
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Green
} else {
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Red
    Write-Host "     ❌ SOME TESTS FAILED (exit code: $exitCode)" -ForegroundColor Red
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Red
}

Write-Host ""
exit $exitCode
