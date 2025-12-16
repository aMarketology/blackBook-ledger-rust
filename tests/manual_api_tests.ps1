# Manual API Test Script for Alice & Bob
# This script demonstrates direct API calls for debugging
#
# Usage: .\manual_api_tests.ps1

$BASE_URL = "http://localhost:1234"

# Test Account Constants
$ALICE = @{
    Address = "L1ALICE000000001"
    PublicKey = "4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57"
    PrivateKey = "616c6963655f746573745f6163636f756e745f76310000000000000000000001"
    Username = "alice_test"
}

$BOB = @{
    Address = "L1BOB0000000001"
    PublicKey = "b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f"
    PrivateKey = "626f625f746573745f6163636f756e745f763100000000000000000000000002"
    Username = "bob_test"
}

function Get-Timestamp {
    return [int][double]::Parse((Get-Date -UFormat %s))
}

function Show-Header {
    param([string]$Title)
    Write-Host ""
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host "  $Title" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host ""
}

function Show-Request {
    param([string]$Method, [string]$Url, $Body = $null)
    Write-Host "📤 $Method $Url" -ForegroundColor Yellow
    if ($Body) {
        Write-Host ($Body | ConvertTo-Json -Depth 10) -ForegroundColor Gray
    }
}

function Show-Response {
    param($Response, [int]$StatusCode = 200)
    if ($StatusCode -eq 200) {
        Write-Host "📥 Response (200 OK):" -ForegroundColor Green
    } else {
        Write-Host "📥 Response ($StatusCode):" -ForegroundColor Red
    }
    Write-Host ($Response | ConvertTo-Json -Depth 10) -ForegroundColor White
}

# ============================================================================
Show-Header "🏥 Health Check"
# ============================================================================

try {
    Show-Request "GET" "$BASE_URL/health"
    $response = Invoke-RestMethod -Uri "$BASE_URL/health" -Method Get
    Show-Response $response
    Write-Host "✅ Server is healthy!" -ForegroundColor Green
} catch {
    Write-Host "❌ Server not responding: $_" -ForegroundColor Red
    Write-Host "Please start the server with: cargo run" -ForegroundColor Yellow
    exit 1
}

# ============================================================================
Show-Header "🔐 Connect Alice's Wallet"
# ============================================================================

$connectAlice = @{
    wallet_address = $ALICE.Address
    username = $ALICE.Username
    public_key = $ALICE.PublicKey
}

Show-Request "POST" "$BASE_URL/auth/connect" $connectAlice
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/auth/connect" -Method Post -Body ($connectAlice | ConvertTo-Json) -ContentType "application/json"
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "🔐 Connect Bob's Wallet"
# ============================================================================

$connectBob = @{
    wallet_address = $BOB.Address
    username = $BOB.Username
    public_key = $BOB.PublicKey
}

Show-Request "POST" "$BASE_URL/auth/connect" $connectBob
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/auth/connect" -Method Post -Body ($connectBob | ConvertTo-Json) -ContentType "application/json"
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "💰 Check Balances"
# ============================================================================

Write-Host "👩 Alice's Balance:" -ForegroundColor Yellow
Show-Request "GET" "$BASE_URL/balance/$($ALICE.Address)"
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/balance/$($ALICE.Address)" -Method Get
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host ""
Write-Host "👨 Bob's Balance:" -ForegroundColor Yellow
Show-Request "GET" "$BASE_URL/balance/$($BOB.Address)"
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/balance/$($BOB.Address)" -Method Get
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "📊 Get Markets"
# ============================================================================

Show-Request "GET" "$BASE_URL/markets"
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/markets" -Method Get
    Write-Host "📥 Found $($response.markets.Count) markets:" -ForegroundColor Green
    foreach ($market in $response.markets | Select-Object -First 5) {
        Write-Host "   • $($market.id): $($market.title)" -ForegroundColor White
    }
    if ($response.markets.Count -gt 5) {
        Write-Host "   ... and $($response.markets.Count - 5) more" -ForegroundColor Gray
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "🎯 Alice Places a YES Bet"
# ============================================================================

$timestamp = Get-Timestamp
$aliceBet = @{
    signature = "sig_alice_$timestamp"
    from_address = $ALICE.Address
    market_id = "pegatron_georgetown_2026"
    option = "YES"
    amount = 25.0
    nonce = $timestamp
    timestamp = $timestamp
}

Show-Request "POST" "$BASE_URL/bet/signed" $aliceBet
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/bet/signed" -Method Post -Body ($aliceBet | ConvertTo-Json) -ContentType "application/json"
    Show-Response $response
} catch {
    $statusCode = $_.Exception.Response.StatusCode.value__
    Write-Host "Error ($statusCode): $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "🎯 Bob Places a NO Bet"
# ============================================================================

$timestamp = Get-Timestamp
$bobBet = @{
    signature = "sig_bob_$timestamp"
    from_address = $BOB.Address
    market_id = "pegatron_georgetown_2026"
    option = "NO"
    amount = 20.0
    nonce = $timestamp
    timestamp = $timestamp
}

Show-Request "POST" "$BASE_URL/bet/signed" $bobBet
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/bet/signed" -Method Post -Body ($bobBet | ConvertTo-Json) -ContentType "application/json"
    Show-Response $response
} catch {
    $statusCode = $_.Exception.Response.StatusCode.value__
    Write-Host "Error ($statusCode): $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "💸 Alice Transfers 10 BB to Bob"
# ============================================================================

$transfer = @{
    from = $ALICE.Address
    to = $BOB.Address
    amount = 10.0
}

Show-Request "POST" "$BASE_URL/transfer" $transfer
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/transfer" -Method Post -Body ($transfer | ConvertTo-Json) -ContentType "application/json"
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "💰 Final Balances"
# ============================================================================

Write-Host "👩 Alice's Balance:" -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/balance/$($ALICE.Address)" -Method Get
    Write-Host "   Balance: $($response.balance) BB" -ForegroundColor White
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host "👨 Bob's Balance:" -ForegroundColor Yellow
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/balance/$($BOB.Address)" -Method Get
    Write-Host "   Balance: $($response.balance) BB" -ForegroundColor White
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "📋 Alice's Bet History"
# ============================================================================

Show-Request "GET" "$BASE_URL/bets/$($ALICE.Address)"
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/bets/$($ALICE.Address)" -Method Get
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "📜 Recent Ledger Transactions"
# ============================================================================

Show-Request "GET" "$BASE_URL/ledger/transactions?limit=5"
try {
    $response = Invoke-RestMethod -Uri "$BASE_URL/ledger/transactions?limit=5" -Method Get
    Show-Response $response
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# ============================================================================
Show-Header "✅ Test Complete!"
# ============================================================================

Write-Host "All manual API tests completed." -ForegroundColor Green
Write-Host ""
Write-Host "Available test scripts:" -ForegroundColor Yellow
Write-Host "  • .\tests\run_alice_bob_tests.ps1     - Run Rust integration tests" -ForegroundColor White
Write-Host "  • .\tests\manual_api_tests.ps1        - This script (manual API calls)" -ForegroundColor White
Write-Host ""
