# BlackBook Betting Endpoint Test Suite
# Run this script to test the /bet/signed endpoint

$ErrorActionPreference = "Continue"
$baseUrl = "http://localhost:1234"
$testsPassed = 0
$testsFailed = 0

function Write-TestHeader {
    param($testName)
    Write-Host "`n========================================" -ForegroundColor Cyan
    Write-Host "TEST: $testName" -ForegroundColor Cyan
    Write-Host "========================================" -ForegroundColor Cyan
}

function Write-Success {
    param($message)
    Write-Host "✅ PASS: $message" -ForegroundColor Green
    $script:testsPassed++
}

function Write-Failure {
    param($message)
    Write-Host "❌ FAIL: $message" -ForegroundColor Red
    $script:testsFailed++
}

# Test 1: Connect Wallet
Write-TestHeader "Connect Wallet"
try {
    $connectBody = @{
        wallet_address = "L1PSTEST123"
        username = "powershell_tester"
    } | ConvertTo-Json

    $response = Invoke-RestMethod -Uri "$baseUrl/auth/connect" -Method POST -ContentType "application/json" -Body $connectBody
    
    if ($response.success -eq $true -and $response.balance -eq 100.0) {
        Write-Success "Wallet connected successfully with 100 BB balance"
        Write-Host "   Wallet: $($response.wallet_address)" -ForegroundColor Gray
    } else {
        Write-Failure "Wallet connection returned unexpected data"
        Write-Host "   Response: $($response | ConvertTo-Json)" -ForegroundColor Yellow
    }
} catch {
    Write-Failure "Wallet connection failed: $($_.Exception.Message)"
}

# Test 2: Place Valid Bet
Write-TestHeader "Place Valid Bet"
try {
    $timestamp = [Math]::Floor([double]::Parse((Get-Date -UFormat %s)))
    
    $betBody = @{
        signature = "sig_test_$timestamp"
        from_address = "L1PSTEST123"
        market_id = "pegatron_georgetown_2026"
        option = "0"
        amount = 10.0
        nonce = 1
        timestamp = $timestamp
    } | ConvertTo-Json

    Write-Host "   Request Body:" -ForegroundColor Gray
    Write-Host "   $betBody" -ForegroundColor DarkGray

    $response = Invoke-RestMethod -Uri "$baseUrl/bet/signed" -Method POST -ContentType "application/json" -Body $betBody
    
    if ($response.success -eq $true -and $response.amount -eq 10.0 -and $response.new_balance -eq 90.0) {
        Write-Success "Bet placed successfully"
        Write-Host "   Bet ID: $($response.bet_id)" -ForegroundColor Gray
        Write-Host "   TX ID: $($response.transaction_id)" -ForegroundColor Gray
        Write-Host "   New Balance: $($response.new_balance) BB" -ForegroundColor Gray
    } else {
        Write-Failure "Bet response unexpected"
        Write-Host "   Response: $($response | ConvertTo-Json)" -ForegroundColor Yellow
    }
} catch {
    Write-Failure "Bet placement failed: $($_.Exception.Message)"
    if ($_.ErrorDetails.Message) {
        Write-Host "   Error Details: $($_.ErrorDetails.Message)" -ForegroundColor Yellow
    }
}

# Test 3: Check Balance
Write-TestHeader "Check Balance After Bet"
try {
    $response = Invoke-RestMethod -Uri "$baseUrl/balance/L1PSTEST123" -Method GET
    
    if ($response.balance -eq 90.0) {
        Write-Success "Balance correct after bet (90 BB)"
    } else {
        Write-Failure "Balance incorrect: $($response.balance) BB (expected 90 BB)"
    }
} catch {
    Write-Failure "Balance check failed: $($_.Exception.Message)"
}

# Test 4: Invalid Option
Write-TestHeader "Test Invalid Option (Should Fail)"
try {
    $timestamp = [Math]::Floor([double]::Parse((Get-Date -UFormat %s)))
    
    $betBody = @{
        signature = "sig_invalid"
        from_address = "L1PSTEST123"
        market_id = "pegatron_georgetown_2026"
        option = "99"  # Invalid
        amount = 5.0
        nonce = 2
        timestamp = $timestamp
    } | ConvertTo-Json

    $response = Invoke-RestMethod -Uri "$baseUrl/bet/signed" -Method POST -ContentType "application/json" -Body $betBody -ErrorAction Stop
    
    Write-Failure "Invalid option should have been rejected"
} catch {
    if ($_.Exception.Response.StatusCode -eq 400) {
        Write-Success "Invalid option correctly rejected with 400 error"
    } else {
        Write-Failure "Invalid option rejected but with wrong status code: $($_.Exception.Response.StatusCode)"
    }
}

# Test 5: Expired Timestamp
Write-TestHeader "Test Expired Timestamp (Should Fail)"
try {
    $oldTimestamp = [Math]::Floor([double]::Parse((Get-Date -UFormat %s))) - (48 * 3600)  # 2 days ago
    
    $betBody = @{
        signature = "sig_expired"
        from_address = "L1PSTEST123"
        market_id = "pegatron_georgetown_2026"
        option = "0"
        amount = 5.0
        nonce = 3
        timestamp = $oldTimestamp
    } | ConvertTo-Json

    $response = Invoke-RestMethod -Uri "$baseUrl/bet/signed" -Method POST -ContentType "application/json" -Body $betBody -ErrorAction Stop
    
    Write-Failure "Expired timestamp should have been rejected"
} catch {
    if ($_.Exception.Response.StatusCode -eq 401) {
        Write-Success "Expired timestamp correctly rejected with 401 error"
    } else {
        Write-Failure "Expired timestamp rejected but with wrong status code: $($_.Exception.Response.StatusCode)"
    }
}

# Test 6: View Market
Write-TestHeader "View Market Details"
try {
    $response = Invoke-RestMethod -Uri "$baseUrl/markets/pegatron_georgetown_2026" -Method GET
    
    if ($response.id -eq "pegatron_georgetown_2026") {
        Write-Success "Market details retrieved"
        Write-Host "   Market: $($response.description)" -ForegroundColor Gray
        Write-Host "   Outcome 0 Pool: $($response.outcome_0_pool) BB" -ForegroundColor Gray
        Write-Host "   Outcome 1 Pool: $($response.outcome_1_pool) BB" -ForegroundColor Gray
    } else {
        Write-Failure "Market details incorrect"
    }
} catch {
    Write-Failure "Market retrieval failed: $($_.Exception.Message)"
}

# Test 7: View Ledger
Write-TestHeader "View Blockchain Ledger"
try {
    $response = Invoke-RestMethod -Uri "$baseUrl/ledger" -Method GET
    
    if ($response.Count -gt 0) {
        Write-Success "Ledger retrieved with $($response.Count) transactions"
        Write-Host "   Recent transactions:" -ForegroundColor Gray
        $response | Select-Object -First 3 | ForEach-Object {
            Write-Host "   - $($_.tx_type): $($_.amount) BB" -ForegroundColor DarkGray
        }
    } else {
        Write-Failure "Ledger is empty"
    }
} catch {
    Write-Failure "Ledger retrieval failed: $($_.Exception.Message)"
}

# Summary
Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "TEST SUMMARY" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "✅ Passed: $testsPassed" -ForegroundColor Green
Write-Host "❌ Failed: $testsFailed" -ForegroundColor Red
Write-Host "Total: $($testsPassed + $testsFailed)" -ForegroundColor White

if ($testsFailed -eq 0) {
    Write-Host "`n🎉 ALL TESTS PASSED! 🎉" -ForegroundColor Green
    exit 0
} else {
    Write-Host "`n⚠️  SOME TESTS FAILED" -ForegroundColor Yellow
    exit 1
}
