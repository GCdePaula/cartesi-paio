# cartesi-paio


## User transactions

Users build an [EIP-712](https://eips.ethereum.org/EIPS/eip-712) signed transaction. A `SignedTransaction` consists of the pair `SigningMessage` and a `Signature`. They then submit it to a sequencer frontend.

Note that this signed transaction contains the address of the destination dapp. It also contains a nonce (which is per application), and the maximum gas price this user is willing to pay _for DA_.


## Sequencer frontend

The frontend is the component in the sequencer that receives user transactions.
Think that, in some frequency, this component yields a bunch of unordered `SignedTransaction`.

There are many kinds of sequencer front end:
* In a centralized sequencer, users send transactions to the sequencer through a “submit endpoint”.
* In a based sequencer, users send transactions to a mempool/p2p-network, and the set of Ethereum block builders running some version of chorizo will pick these transactions up.
* In Espresso, users send transactions to the Espresso network, which is then picked up by the sequencers, and finally the elected builder has the rights to sequence these transactions.


## Sequencer batcher

The batcher is the component in the sequencer that takes an unordered set of `SignedTransaction`s, and builds a `Batch`, consisting of an ordered set of `WireTransaction`s.
Here, the sequencer has the freedom to do anything (except forge signatures, since it’s cryptographically impossible).
They can reorder transactions, compress them, aggregate signatures, whatever.

As an important detail, a `Batch` contains a single “payment address”, which we tacitly assume is the wallet of the sequencer who created the batch.
Also, note that a `Batch` contains transactions with different destination apps/addresses.

The important part is that, by looking just at a `Batch` and perhaps some context, a `Batch` can be parsed and verified into a list of `Transaction`s.
The simplest batcher orders the user signed transactions in a first come first serve policy, and just appends and serializes all transactions into a blob.


## Sequencer backend

The backend is the component in the sequencer that takes a `Batch` and submits it to a DA layer. It could in Ethereum calldata, Ethereum 4844 blob, Espresso DA, Avail, Celestia, etc.


## Batch parser library

These batches will make their way to all apps, which contain a batch parser.
The parser will read a batch, verify that it is valid (i.e. signatures and nonces match), and return an ordered list of `Transaction`s.
Note that the parser exists inside the Cartesi machine; it is compiled to RISC-V.


## Payment app

One of these apps is special: it is the payment app.
This app is written by us, we run a validator to protect it, and has our seal of approval. Sequencers must trust this app, but not any of the others.
This app also has a batch parser, and a wallet.

Users will deposit money on this app, which will pay for DA costs incurred by the sequencer.
After parsing a batch, this app will transfer ether from each user that submitted a transaction to the sequencer wallet (i.e. “payment address”), using the maximum gas price signed by the user, and the size of the transaction payload.

As an implementation detail, we decided to accept transaction from users without funds (i.e. it’s the sequencer’s fault for batching this transaction).
