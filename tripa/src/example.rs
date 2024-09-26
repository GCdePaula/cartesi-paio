#![feature(async_closure)]
use alloy_core::primitives::Address;
use hex;
use message::{
    AppNonces, Batch, BatchBuilder, SignedTransaction, WalletState, DOMAIN,
};
use serde_json::json;

mod utils;

use std::io;

fn main() -> io::Result<()> {
    let tx = r#"{"message":{"app":"0x0000000000000000000000000000000000000000","nonce":0,"max_gas_price":0,"data":"0x48656c6c6f2c20576f726c6421"},"signature":{"r":"0xfa6f7fd6825c953b355c8970fd2c9322162987bfb6898aa78f74f2be6bf8b10c","s":"0x9a2018a7e31b623a91802147e6f8d5c658e17191e69f6663052efda71db72e2","yParity":"0x1"}}"#;

    let signed_tx: SignedTransaction = serde_json::from_str(tx).unwrap();

    let mut batch_builder = BatchBuilder::new(Address::ZERO);
    batch_builder.add(signed_tx);

    let batch = batch_builder.build();
    let encoded_batch = batch.to_bytes();
    let hex_batch = hex::encode(encoded_batch);
    println!("{}", hex_batch);
    Ok(())
}
