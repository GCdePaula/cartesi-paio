#![feature(async_closure)]
use hex;
use message::{AppNonces, Batch, BatchBuilder, WalletState, DOMAIN};
use serde_json::json;

mod utils;

use std::io;

fn main() -> io::Result<()> {
    let mut hex_input = String::new();
    let input_size = io::stdin().read_line(&mut hex_input)?;
    println!("{input_size} bytes read");
    println!("{}", hex_input.clone());

    hex_input.pop(); // remove end of line \n

    let input = match hex::decode(hex_input) {
        Ok(i) => i,
        Err(e) => {
            println!("Could not decode hex:\n{e}");
            return Ok(());
        }
    };

    let maybe_batch = Batch::from_bytes(input.as_slice());
    match maybe_batch {
        Ok(batch) => {
            let deserialized = json!(batch);
            println!("Batch is:\n{:?}", deserialized);
        }
        Err(e) => {
            println!("Not a proper batch: {}", e);
        }
    }

    Ok(())
}
