use std::collections::HashMap;

use alloy_core::{
    primitives::{Address, SignatureError, U256},
    sol,
    sol_types::{eip712_domain, Eip712Domain, SolStruct},
};
use alloy_signer::Signature;

use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use derive_more::{Display, Into};
use serde::{Deserialize, Serialize};
pub struct WalletState {
    pub domain: Eip712Domain,

    // app address to app state
    pub app_nonces: HashMap<Address, AppNonces>,

    // user address to balance
    pub balances: HashMap<Address, U256>,
}

impl WalletState {
    pub fn verify_batch(&mut self, batch: Batch) -> Vec<Transaction> {
        batch
            .txs
            .iter()
            .filter_map(|tx| self.verify_single(batch.sequencer_payment_address, tx))
            .collect()
    }
    // TODO: create custom error type in order to explain why it did not work
    pub fn verify_single(
        &mut self,
        sequencer_payment_address: Address,
        tx: &WireTransaction,
    ) -> Option<Transaction> {
        let app_nonce = self.app_nonces.entry(tx.app).or_default();
        let tx_opt = app_nonce.verify_tx(tx, &self.domain);

        if let Some(ref tx) = tx_opt {
            let cost_opt = tx.cost();
            let payment = if let Some(cost) = cost_opt {
                self.withdraw_forced(tx.sender, cost)
            } else {
                self.withdraw_forced(tx.sender, U256::MAX)
            };
            self.deposit(sequencer_payment_address, payment);
        }

        tx_opt
    }

    pub fn verify_raw_batch(&mut self, raw_batch: &[u8]) -> postcard::Result<Vec<Transaction>> {
        let batch = Batch::from_bytes(raw_batch)?;
        Ok(self.verify_batch(batch))
    }

    pub fn deposit(&mut self, user: Address, value: U256) {
        let balance = self.balances.entry(user).or_default();
        *balance += value;
    }

    pub fn withdraw_forced(&mut self, user: Address, value: U256) -> U256 {
        let balance = self.balances.entry(user).or_default();
        if *balance < value {
            let prev = *balance;
            *balance = U256::ZERO;
            prev
        } else {
            *balance -= value;
            value
        }
    }
}

impl WalletState {
    pub fn new() -> Self {
        WalletState {
            domain: DOMAIN.clone(),
            app_nonces: HashMap::new(),
            balances: HashMap::new(),
        }
    }
    pub fn add_app_nonce(&mut self, address: Address, nonces: AppNonces) {
        self.app_nonces.insert(address, nonces);
    }
}

pub struct AppState {
    pub domain: Eip712Domain,
    pub address: Address,
    pub nonces: AppNonces,
}

impl AppState {
    pub fn verify_batch(&mut self, batch: Batch) -> Vec<Transaction> {
        batch
            .txs
            .iter()
            .filter_map(|tx| {
                if self.address != tx.app {
                    return None;
                }

                self.nonces.verify_tx(tx, &self.domain)
            })
            .collect()
    }

    pub fn verify_raw_batch(&mut self, raw_batch: &[u8]) -> postcard::Result<Vec<Transaction>> {
        let batch = Batch::from_bytes(raw_batch)?;
        Ok(self.verify_batch(batch))
    }
}

pub struct AppNonces {
    // user address to nonce
    pub nonces: HashMap<Address, u64>,
}

impl AppNonces {
    pub fn new() -> Self {
        AppNonces {
            nonces: HashMap::new(),
        }
    }
    pub fn set_nonce(&mut self, address: Address, value: u64) {
        self.nonces.insert(address, value);
    }
    pub fn get_nonce(&self, address: &Address) -> Option<&u64> {
        self.nonces.get(address)
    }
    pub fn verify_tx(
        &mut self,
        tx: &WireTransaction,
        domain: &Eip712Domain,
    ) -> Option<Transaction> {
        let Some(tx) = tx.verify(&domain) else {
            return None;
        };

        let expected_nonce = self.nonces.entry(tx.sender).or_insert(0);

        if *expected_nonce != tx.nonce {
            return None;
        }

        *expected_nonce += 1;
        Some(tx)
    }
}

impl Default for AppNonces {
    fn default() -> Self {
        Self {
            nonces: HashMap::new(),
        }
    }
}

pub struct Transaction {
    pub sender: Address,
    pub app: Address,
    pub nonce: u64,
    pub max_gas_price: u128,

    pub data: Vec<u8>,
}

impl Transaction {
    pub fn cost(&self) -> Option<U256> {
        U256::checked_mul(U256::from(self.max_gas_price), U256::from(self.data.len()))
    }
}

sol! {
   #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct SigningMessage {
        address app;
        uint64 nonce;
        uint128 max_gas_price;
        bytes data;
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct WireTransaction {
    pub app: Address,
    pub nonce: u64,
    pub max_gas_price: u128,
    pub data: Vec<u8>,
    pub signature: Signature,
}

impl WireTransaction {
    pub fn from_signed_transaction(value: &SignedTransaction) -> Self {
        Self {
            app: value.message.app,
            nonce: value.message.nonce,
            max_gas_price: value.message.max_gas_price,
            data: value.message.data.to_vec(),
            signature: value.signature,
        }
    }

    pub fn to_signed_transaction(&self) -> SignedTransaction {
        SignedTransaction {
            message: SigningMessage {
                app: self.app,
                nonce: self.nonce,
                max_gas_price: self.max_gas_price,
                data: self.data.clone().into(),
            },
            signature: self.signature,
        }
    }

    pub fn verify(&self, domain: &Eip712Domain) -> Option<Transaction> {
        let Ok(sender) = self.to_signed_transaction().recover(domain) else {
            return None;
        };

        Some(Transaction {
            sender,
            app: self.app,
            nonce: self.nonce,
            max_gas_price: self.max_gas_price,
            data: self.data.clone(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct Batch {
    pub sequencer_payment_address: Address,
    pub txs: Vec<WireTransaction>,
}

impl Batch {
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(&self).unwrap()
    }

    pub fn from_bytes(bytes: &[u8]) -> postcard::Result<Self> {
        postcard::from_bytes(bytes)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct BatchBuilder {
    pub sequencer_payment_address: Address,
    pub txs: Vec<SignedTransaction>,
}

impl BatchBuilder {
    pub fn new(sequencer_payment_address: Address) -> Self {
        Self {
            sequencer_payment_address,
            txs: Vec::new(),
        }
    }

    pub fn add(&mut self, tx: SignedTransaction) {
        self.txs.push(tx)
    }

    pub fn build(self) -> Batch {
        let txs = self
            .txs
            .iter()
            .map(WireTransaction::from_signed_transaction)
            .collect();

        Batch {
            sequencer_payment_address: self.sequencer_payment_address,
            txs,
        }
    }
}

#[derive(
    Serialize,
    Deserialize,
    Ord,
    Display,
    PartialOrd,
    PartialEq,
    Eq,
    Hash,
    Debug,
    CanonicalDeserialize,
    CanonicalSerialize,
    Default,
    Clone,
    Copy,
    Into,
)]
#[display(fmt = "{_0}")]
pub struct NamespaceId(u64);

impl From<u64> for NamespaceId {
    fn from(number: u64) -> Self {
        Self(number)
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub struct EspressoTransaction {
    namespace: NamespaceId,
    #[serde(with = "base64_bytes")]
    payload: Vec<u8>,
}

impl EspressoTransaction {
    pub fn new(namespace: NamespaceId, payload: Vec<u8>) -> Self {
        Self { namespace, payload }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct SignedTransaction {
    pub message: SigningMessage,
    pub signature: Signature,
}


impl SignedTransaction {
    pub fn valdiate(&self, domain: &Eip712Domain) -> bool {
        self.recover(domain).is_ok()
    }

    pub fn recover(
        &self,
        domain: &Eip712Domain,
    ) -> Result<Address, SignatureError> {
        let signing_hash = self.message.eip712_signing_hash(&domain);
        self.signature.recover_address_from_prehash(&signing_hash)
    }

    pub fn to_wire_transaction(&self) -> WireTransaction {
        WireTransaction {
            app: self.message.app,
            nonce: self.message.nonce,
            max_gas_price: self.message.max_gas_price,
            data: self.message.data.clone().into(),
            signature: self.signature,
        }
    }
}

pub const DOMAIN: Eip712Domain = eip712_domain!(
   name: "CartesiPaio",
   version: "0.0.1",
   chain_id: 1115511112,
   verifying_contract: Address::ZERO,
);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct SubmitPointTransaction {
    pub message: String,
    pub signature: String
}

#[cfg(test)]
mod tests {
    use alloy_core::sol_types::SolStruct;
    use alloy_signer::SignerSync;
    use alloy_signer_wallet::LocalWallet;
    use std::str::FromStr;

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
        let signer = LocalWallet::from_str(
            "8114fae7aa0a92c7e3a6015413a54539b4ba9f28254a70f67a3969d73c33509b",
        )
        .unwrap();
        assert_eq!(
            alloy_core::hex::encode(signer.to_field_bytes()),
            "8114fae7aa0a92c7e3a6015413a54539b4ba9f28254a70f67a3969d73c33509b"
        );
        assert_eq!(
            "0x7306897365c277A6951FDA9519fD0CCc16341E4A",
            signer.address().to_string()
        );

        let signature = signer.sign_typed_data_sync(&v, &DOMAIN).unwrap();
        assert_eq!(
            r#"{"r":"0xfa6f7fd6825c953b355c8970fd2c9322162987bfb6898aa78f74f2be6bf8b10c","s":"0x9a2018a7e31b623a91802147e6f8d5c658e17191e69f6663052efda71db72e2","yParity":"0x1"}"#,
            serde_json::to_string(&signature).unwrap()
        );
        let signed_tx = SignedTransaction {
            message: v,
            signature,
        };

        let ret = serde_json::to_string(&signed_tx).unwrap();

        assert_eq!(
            r#"{"message":{"app":"0x0000000000000000000000000000000000000000","nonce":0,"max_gas_price":0,"data":"0x48656c6c6f2c20576f726c6421"},"signature":{"r":"0xfa6f7fd6825c953b355c8970fd2c9322162987bfb6898aa78f74f2be6bf8b10c","s":"0x9a2018a7e31b623a91802147e6f8d5c658e17191e69f6663052efda71db72e2","yParity":"0x1"}}"#,
            ret
        );

        (ret, signer.address())
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

        assert_eq!(
            r#"{"name":"CartesiPaio","version":"0.0.1","chainId":"0x539","verifyingContract":"0x0000000000000000000000000000000000000000"}"#,
            serde_json::to_string(&DOMAIN).unwrap()
        );
    }
}
