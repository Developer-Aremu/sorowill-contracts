#![cfg(test)]

//! Regression tests for issues #596, #597, #598 and #599.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env,
};

use crate::{
    migration, migration_rules, storage, Allocation, Beneficiary, WillContract,
    WillContractClient, WillStatus,
};

/// Creates a funded single-beneficiary will with `guardian_count` guardians
/// (threshold 1 when any exist). Returns
/// `(env, contract_id, owner, token, will_id, guardians)`.
fn setup(
    guardian_count: u32,
) -> (
    Env,
    Address,
    Address,
    Address,
    u64,
    soroban_sdk::Vec<Address>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let owner = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    StellarAssetClient::new(&env, &token).mint(&owner, &1_000_000);

    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    let mut guardians: soroban_sdk::Vec<Address> = soroban_sdk::Vec::new(&env);
    for _ in 0..guardian_count {
        guardians.push_back(Address::generate(&env));
    }
    let threshold: u32 = if guardians.is_empty() { 0 } else { 1 };

    let will_id = client.create_will(
        &owner,
        &vec![&env, (token.clone(), 1_000_000_i128)],
        &vec![
            &env,
            Beneficiary {
                address: Address::generate(&env),
                allocation: Allocation::Percentage(10_000),
            },
        ],
        &90,
        &7,
        &guardians,
        &threshold,
        &None,
        &0,
    );
    (env, contract_id, owner, token, will_id, guardians)
}

// ---------------------------------------------------------------------------
// #596: migration invariants
// ---------------------------------------------------------------------------

/// Loads the stored will, forces it to `version` and returns it.
fn stale_will(
    env: &Env,
    contract_id: &Address,
    will_id: u64,
    version: u32,
) -> crate::Will {
    env.as_contract(contract_id, || {
        let mut will = match storage::load_will(env, will_id) {
            Ok(will) => will,
            Err(_) => panic!("will not found"),
        };
        will.schema_version = version;
        will
    })
}

#[test]
fn migration_accepts_a_consistent_v0_will() {
    let (env, contract_id, _owner, _token, will_id, _gs) = setup(0);
    let will = stale_will(&env, &contract_id, will_id, 0);
    let upgraded = env.as_contract(&contract_id, || migration::upgrade(&env, will));
    assert_eq!(upgraded.schema_version, migration::CURRENT_SCHEMA_VERSION);
}

#[test]
fn migration_rules_reject_triggered_will_without_trigger_time() {
    let (env, contract_id, _owner, _token, will_id, _gs) = setup(0);
    let mut will = stale_will(&env, &contract_id, will_id, 1);
    assert!(migration_rules::is_valid_migration(&will, 0));

    will.status = WillStatus::Triggered;
    will.trigger_time = None;
    assert!(!migration_rules::is_valid_migration(&will, 0));
}

#[test]
fn migration_rules_reject_non_sequential_version_and_oversized_tally() {
    let (env, contract_id, _owner, _token, will_id, _gs) = setup(0);
    let mut will = stale_will(&env, &contract_id, will_id, 1);
    // Version jumped by two.
    assert!(!migration_rules::is_valid_migration(&will, 5));
    // Tally larger than the (empty) guardian list.
    will.guardian_votes = 3;
    assert!(!migration_rules::is_valid_migration(&will, 0));
}

#[test]
#[should_panic]
fn upgrade_rejects_partially_migrated_triggered_state() {
    let (env, contract_id, _owner, _token, will_id, _gs) = setup(0);
    let mut will = stale_will(&env, &contract_id, will_id, 0);
    will.status = WillStatus::Triggered;
    will.trigger_time = None;
    env.as_contract(&contract_id, || migration::upgrade(&env, will));
}
