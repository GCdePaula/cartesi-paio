# Paio

Paio is a Sequencer SDK that provides a suite of libraries for building sequencers for application-specific rollups.
It streamlines the process of receiving, batching, and submitting user transactions to Data Availability (DA) layers.
Through an integrated payment application, users can pay for the DA costs incurred during transaction processing.

## Concepts

### User transactions

Users build an [EIP-712](https://eips.ethereum.org/EIPS/eip-712) signed transaction.
A `SignedTransaction` consists of the pair `SigningMessage` and a `Signature`.
Users then submit this transaction to a sequencer frontend.

The signed transaction includes:

- The address of the destination dApp.
- A nonce (specific to each application).
- The maximum gas price the user is willing to pay for Data Availability (DA).


### Sequencer frontend

The sequencer frontend is the component that receives user transactions.
At regular intervals, it produces a list of `SignedTransaction`s.

There are various types of sequencer frontends:

- **Centralized Sequencer**: Users send transactions directly to the sequencer through a submission endpoint.
- **Based Sequencer**: Users send transactions to a mempool or peer-to-peer network, where Ethereum block builders running compatible software will pick them up.
- **Espresso Sequencer**: Users send transactions to the Espresso network, where sequencers collect them. An elected builder then sequences these transactions.


### Sequencer Batcher

The batcher component takes a list of `SignedTransaction`s and builds a `Batch`, which consists of an ordered set of `WireTransaction`s.

The sequencer has the freedom to compress transactions, aggregate signatures, and reorder transactions (though ideally, they should maintain the original order).

A `Batch` includes a single **payment address**, assumed to be the wallet of the sequencer who created the batch.
A `Batch` can contain transactions destined for different applications.

The critical aspect is that a `Batch` can be parsed and verified into a list of `Transaction`s by anyone (possibly with some additional context)
A simple batcher might order user transactions on a first-come, first-served basis and serialize them into a blob.


### Sequencer Backend

The backend component takes a `Batch` and submits it to a DA layer, such as:

- Ethereum calldata
- Ethereum EIP-4844
- Espresso DA
- Avail DA
- Celestia
- EigenDA


### Applications

Batches are received by all applications.
Each application should use the batch parser library to parse and validate transactions.
The parser reads a batch, verifies its validity (checking signatures and nonces), and returns an ordered list of `Transaction`s.
The parser operates within the Cartesi machine and is compiled to RISC-V.


### Payment Application

One of the dApps is special: the **payment application**.
This app is developed and validated by us and carries our seal of approval.
Sequencers must trust this app but not necessarily any others.

Key features of the payment app:

- Includes a batch parser and a wallet.
- Users deposit funds into this app to cover DA costs incurred by the sequencer.
- After parsing a batch, the app transfers Ether from each user who submitted a transaction to the sequencer's wallet (the **payment address**). The amount is calculated based on the DA layer's data price (capped at the maximum gas price specified by the user) and the size of the transaction payload.

As a current important implementation detail, we accept transactions from users without sufficient funds.
In such cases, it's the sequencer's responsibility for including these transactions in the batch.
This may change in the future.



## `tripa` service

Tripa is a sequencer implementation using the Paio SDK.
It is a centralized sequencer that submits transactions to Ethereum as calldata.

It exposes the following endpoints:
* `GET /nonce`: get user nonce.
* `GET /domain`: get the domain.
* `GET /gas`: get gas price.
* `POST /transaction`: post a transaction
* `GET /batch`: get current batch

## `message` lib

The `message` crate contains basic types definitions
It also implements batch encoding/decoding, and signature and nonce verification.

Batches are currently encoded using the [`postcard` crate](https://crates.io/crates/postcard).
The crate offers the `AppState` type that can be used to validate signatures and nonces.
This type can be used like this:

```rust
use message::AppState;

let raw_batch = ...; // obtain raw batch from eg libcmt.

let mut app_state = AppState::new(DOMAIN, Address::ZERO);
let batch = app_state
    .verify_raw_batch(&raw_batch)
    .expect("failed to parse batch");

for tx in batch {
    println!("{:?}", tx);
}
```
