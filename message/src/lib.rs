use alloy_core::{
    primitives::Address,
    sol,
    sol_types::{eip712_domain, Eip712Domain},
};
use alloy_signer::Signature;
use serde::{Deserialize, Serialize};

pub struct Transaction {
    pub sender: Address,
    pub app: Address,
    pub nonce: u64,
    pub max_gas_price: u64,

    pub data: Vec<u8>,
}

sol! {
   #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct SigningMessage {
        address app;
        uint64 nonce;
        uint64 max_gas_price;
        bytes data;
    }
}

pub type WireTransaction = SigningMessage; // TODO

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SignedTransaction {
    pub message: SigningMessage,
    pub signature: Signature,
}

pub const DOMAIN: Eip712Domain = eip712_domain!(
   name: "CartesiPaio",
   version: "0.0.1",
   chain_id: 1337,
   verifying_contract: Address::ZERO,
);

#[cfg(test)]
mod tests {
    use alloy_core::sol_types::SolStruct;
    use alloy_signer::SignerSync;
    use alloy_signer_wallet::LocalWallet;

    use super::*;

    fn produce_tx() -> (String, Address) {
        let json = r#"
        {
            "app":"0x0000000000000000000000000000000000000000",
            "nonce":0,
            "max_gas_price":0,
            "data":"0x48656c6c6f2c20576f726c6421"
        }
        "#;

        let v: SigningMessage = serde_json::from_str(json).unwrap();
        let signer = LocalWallet::random();
        let signature = signer.sign_typed_data_sync(&v, &DOMAIN).unwrap();
        let signed_tx = SignedTransaction {
            message: v,
            signature,
        };

        (serde_json::to_string(&signed_tx).unwrap(), signer.address())
    }

    #[test]
    fn test() {
        let (tx_json, signer) = produce_tx(); // metamask
        println!("JSON: {tx_json}");

        let tx: SignedTransaction = serde_json::from_str(&tx_json).unwrap();
        let signing_hash = tx.message.eip712_signing_hash(&DOMAIN);
        let recovered = tx
            .signature
            .recover_address_from_prehash(&signing_hash)
            .unwrap();

        assert_eq!(signer, recovered);
    }
}
