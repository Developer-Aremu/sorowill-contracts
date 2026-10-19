#![cfg(test)]

//! Regression tests for issues #592-#595.

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec, Address, Env, Vec as SorobanVec,
};

use crate::{
    balance_consistency, Allocation, Beneficiary, WillContract, WillContractClient, WillStatus,
};

const DAY: u64 = 86_400;

/// Registers a fresh Stellar asset token and mints `amount` of it to `to`.
fn new_token(env: &Env, admin: &Address, to: &Address, amount: i128) -> Address {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let token_address = sac.address();
    StellarAssetClient::new(env, &token_address).mint(to, &amount);
    token_address
}

fn setup<'a>() -> (Env, WillContractClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);
    let owner = Address::generate(&env);
    let primary = new_token(&env, &owner, &owner, 1_000_000_000);
    let contract_id = env.register(WillContract, ());
    let client = WillContractClient::new(&env, &contract_id);
    (env, client, owner, primary)
}

fn one_beneficiary(env: &Env) -> SorobanVec<Beneficiary> {
    vec![
        env,
        Beneficiary {
            address: Address::generate(env),
            allocation: Allocation::Percentage(10_000),
        },
    ]
}

// ---------------------------------------------------------------------------
// #592: balance == balances[primary_token]
// ---------------------------------------------------------------------------

#[test]
fn issue_592_top_ups_keep_mirror_and_map_in_sync() {
    let (env, client, owner, primary) = setup();
    let secondary = new_token(&env, &owner, &owner, 1_000_000_000);
    let tokens: SorobanVec<(Address, i128)> = vec![&env, (primary.clone(), 1_000_i128)];
    let will_id = client.create_will(
        &owner,
        &tokens,
        &one_beneficiary(&env),
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    client.top_up(&will_id, &owner, &secondary, &500);
    let will = client.get_will(&will_id);
    assert!(balance_consistency::is_consistent(&will));
    assert_eq!(will.balance, 1_000);

    client.top_up(&will_id, &owner, &primary, &250);
    let will = client.get_will(&will_id);
    assert!(balance_consistency::is_consistent(&will));
    assert_eq!(will.balance, 1_250);
    assert_eq!(will.balances.get(primary.clone()).unwrap(), 1_250);
    assert_eq!(will.balances.get(secondary.clone()).unwrap(), 500);
}

#[test]
fn issue_592_state_reads_between_top_up_and_checkin_stay_consistent() {
    let (env, client, owner, primary) = setup();
    let secondary = new_token(&env, &owner, &owner, 1_000_000_000);
    let tokens: SorobanVec<(Address, i128)> = vec![&env, (primary.clone(), 2_000_i128)];
    let will_id = client.create_will(
        &owner,
        &tokens,
        &one_beneficiary(&env),
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );

    env.ledger().with_mut(|l| l.timestamp += 91 * DAY);
    client.trigger_will(&will_id);
    client.emergency_checkin(&will_id, &owner);
    let will = client.get_will(&will_id);
    assert_eq!(will.status, WillStatus::Active);
    assert!(balance_consistency::is_consistent(&will));

    client.top_up(&will_id, &owner, &secondary, &1);
    assert!(balance_consistency::is_consistent(&client.get_will(&will_id)));
}

#[test]
fn issue_592_sync_heals_a_stale_mirror() {
    let (env, client, owner, primary) = setup();
    let tokens: SorobanVec<(Address, i128)> = vec![&env, (primary, 700_i128)];
    let will_id = client.create_will(
        &owner,
        &tokens,
        &one_beneficiary(&env),
        &90,
        &7,
        &vec![&env],
        &0,
        &None,
        &0,
    );
    let mut will = client.get_will(&will_id);
    will.balance = 1;
    assert!(!balance_consistency::is_consistent(&will));
    balance_consistency::sync_primary_balance(&mut will);
    assert!(balance_consistency::is_consistent(&will));
    assert_eq!(will.balance, 700);
}
