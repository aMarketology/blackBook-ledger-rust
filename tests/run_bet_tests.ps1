# BlackBook L2 Test Suite
# Run this in a separate terminal while the server is running

Write-Host "🧪 BlackBook L2 Test Suite" -ForegroundColor Cyan
Write-Host ("=" * 50)

$baseUrl = "http://localhost:1234"

# Test 1: Connect Alice
Write-Host "`n📝 Test 1: Connect Alice..." -ForegroundColor Yellow
try {
    $alice = Invoke-RestMethod -Uri "$baseUrl/auth/connect" -Method POST -ContentType "application/json" -Body '{"wallet_address":"L1ALICE000000001","public_key":"4013e5a935e9873a57879c471d5da838a0c9c762eea3937eb3cd34d35c97dd57"}'
    Write-Host "Alice connected: $($alice.success)" -ForegroundColor Green
    Write-Host "Balance: $($alice.balance) BB"
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Test 2: Connect Bob  
Write-Host "`n📝 Test 2: Connect Bob..." -ForegroundColor Yellow
try {
    $bob = Invoke-RestMethod -Uri "$baseUrl/auth/connect" -Method POST -ContentType "application/json" -Body '{"wallet_address":"L1BOB0000000001","public_key":"b9e9c6a69bf6051839c86115d89788bd9559ab4e266f43e18781ded28ce5573f"}'
    Write-Host "Bob connected: $($bob.success)" -ForegroundColor Green
    Write-Host "Balance: $($bob.balance) BB"
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Get current timestamp
$ts = [int64](Get-Date -UFormat %s)

# Test 3: Alice places bet on texas_sb2420
Write-Host "`n🎲 Test 3: Alice bets 100 BB on YES for texas_sb2420..." -ForegroundColor Yellow
try {
    $aliceBet = @{
        signature = "test_sig_alice_001"
        from_address = "L1ALICE000000001"
        market_id = "texas_sb2420"
        option = "YES"
        amount = 100
        nonce = 1
        timestamp = $ts
    } | ConvertTo-Json
    
    $betResult = Invoke-RestMethod -Uri "$baseUrl/bet/signed" -Method POST -ContentType "application/json" -Body $aliceBet
    if ($betResult.success) {
        Write-Host "Bet success: True" -ForegroundColor Green
        Write-Host "TX: $($betResult.transaction_id)"
        Write-Host "New Balance: $($betResult.new_balance) BB"
    } else {
        Write-Host "Bet failed: $($betResult.error)" -ForegroundColor Red
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Test 4: Bob places bet on NO
Write-Host "`n🎲 Test 4: Bob bets 50 BB on NO for texas_sb2420..." -ForegroundColor Yellow
try {
    $bobBet = @{
        signature = "test_sig_bob_001"
        from_address = "L1BOB0000000001"
        market_id = "texas_sb2420"
        option = "NO"
        amount = 50
        nonce = 1
        timestamp = $ts
    } | ConvertTo-Json
    
    $betResult2 = Invoke-RestMethod -Uri "$baseUrl/bet/signed" -Method POST -ContentType "application/json" -Body $bobBet
    if ($betResult2.success) {
        Write-Host "Bet success: True" -ForegroundColor Green
        Write-Host "TX: $($betResult2.transaction_id)"
        Write-Host "New Balance: $($betResult2.new_balance) BB"
    } else {
        Write-Host "Bet failed: $($betResult2.error)" -ForegroundColor Red
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Test 5: Check Ledger
Write-Host "`n📒 Test 5: Check Ledger Transactions..." -ForegroundColor Yellow
try {
    $ledger = Invoke-RestMethod -Uri "$baseUrl/ledger/transactions?limit=10" -Method GET
    Write-Host "Total transactions: $($ledger.stats.total_transactions)" -ForegroundColor Cyan
    Write-Host "Total bets: $($ledger.stats.total_bets)"
    Write-Host "Bet volume: $($ledger.stats.bet_volume) BB"
    
    Write-Host "`nRecent Transactions:" -ForegroundColor Cyan
    foreach ($tx in $ledger.transactions) {
        $market = if ($tx.market_id) { " on $($tx.market_id)" } else { "" }
        Write-Host "  [$($tx.tx_type)] $($tx.from) -> $($tx.amount) BB$market" -ForegroundColor Gray
    }
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

# Test 6: Check Alice's balance
Write-Host "`n💰 Test 6: Check balances..." -ForegroundColor Yellow
try {
    $aliceBalance = Invoke-RestMethod -Uri "$baseUrl/balance/L1ALICE000000001" -Method GET
    Write-Host "Alice balance: $($aliceBalance.balance) BB" -ForegroundColor Cyan
    
    $bobBalance = Invoke-RestMethod -Uri "$baseUrl/balance/L1BOB0000000001" -Method GET
    Write-Host "Bob balance: $($bobBalance.balance) BB" -ForegroundColor Cyan
} catch {
    Write-Host "Error: $_" -ForegroundColor Red
}

Write-Host "`n✅ Tests complete!" -ForegroundColor Green
