use std::io::{self, Read, Write};
use std::result::Result;

use message::Batch;

fn to_json_string(str: &str) -> Result<String, Box<dyn std::error::Error>> {
    let batch = Batch::from_bytes(&alloy_core::hex::decode(str.as_bytes())?)?;
    Ok(serde_json::to_string(&batch)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdin = io::stdin();
    let mut buffer = String::new();
    stdin.read_to_string(&mut buffer)?;

    let j = to_json_string(buffer.trim_end())?;

    let mut stdout = io::stdout();
    stdout.write_all(j.as_bytes())?;
    stdout.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let j
            = to_json_string("0x1463f9725f107358c9115bc9d86c72dd5823e9b1e60114ab7528bb862fb57e8a2bcd567a2e929a0be56a5e000a0d48656c6c6f2c20576f726c643f2076a270f52ade97cd95ef7be45e08ea956bfdaf14b7fc4f8816207fa9eb3a5c17207ccdd94ac1bd86a749b66526fff6579e2b6bf1698e831955332ad9d5ed44da7208000000000000001c").unwrap();

        assert_eq!(
            j,
            r#"{"sequencer_payment_address":"0x63F9725f107358c9115BC9d86c72dD5823E9B1E6","txs":[{"app":"0xab7528bb862fB57E8A2BCd567a2e929a0Be56a5e","nonce":0,"max_gas_price":10,"data":[72,101,108,108,111,44,32,87,111,114,108,100,63],"signature":{"r":"0x76a270f52ade97cd95ef7be45e08ea956bfdaf14b7fc4f8816207fa9eb3a5c17","s":"0x7ccdd94ac1bd86a749b66526fff6579e2b6bf1698e831955332ad9d5ed44da72","v":"0x1c"}}]}"#
        );
        println!("{:?}", j);
    }
}
