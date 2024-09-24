#![feature(async_closure)]
use alloy_core::{
    primitives::{address, Address, Bytes, U256},
    sol,
    sol_types::Eip712Domain,
};
use alloy_network::EthereumSigner;
use alloy_node_bindings::Anvil;
use alloy_node_bindings::AnvilInstance;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::TransactionRequest;
use alloy_signer::k256::ecdsa;
use alloy_signer_wallet::LocalWallet;
use alloy_signer_wallet::Wallet;
use anyhow::Error;
use avail_rust::{avail, AvailExtrinsicParamsBuilder, Data, Keypair, SecretUri, WaitFor, SDK};
use axum::{
    extract::State,
    http::StatusCode, http::Method,
    routing::{get, post},
    Json, Router,
};
use celestia_rpc::BlobClient;
use celestia_types::blob::GasPrice;
use celestia_types::nmt::Namespace;
use celestia_types::Blob;
use es_version::SequencerVersion;
use message::{
    AppNonces, BatchBuilder, EspressoTransaction, SignedTransaction, WalletState, WireTransaction,
    DOMAIN,
};
use reqwest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;
use toml;
const USE_LOCAL_ANVIL: bool = false;
const DEPLOY_INPUT_BOX: bool = true;

async fn fund_sequencer(
    signer_address: Address,
    sequencer_address: Address,
    provider: Box<dyn Provider<alloy_transport_http::Http<reqwest::Client>>>,
) {
    let tx = TransactionRequest::default()
        .from(signer_address)
        .to(sequencer_address)
        .value("30000000000000000000".parse().unwrap());
    // Send the transaction and wait for the broadcast.
    let pending_tx = provider.send_transaction(tx).await.unwrap();
    // Wait for the transaction to be included and get the receipt.
    let _receipt = pending_tx.get_receipt().await.unwrap();
}

// TODO: unify this code. the current problem is that provider does not have a size
//       known at compile time.
// async fn deploy_input_box(
//     signer_address: Address,
//     sequencer_address: Address,
//     provider: Box<dyn Provider<alloy_transport_http::Http<reqwest::Client>>>,
// ) -> Address {
//     let nonce = provider
//         .get_transaction_count(signer_address)
//         .await
//         .unwrap();
//     InputBox::deploy_builder(provider)
//         .nonce(nonce)
//         .from(signer_address)
//         .deploy()
//         .await
//         .unwrap()
// }

sol! {
    function EvmAdvance(
        uint256 chainId,
        address appContract,
        address msgSender,
        uint256 blockNumber,
        uint256 blockTimestamp,
        uint256 prevRandao,
        uint256 index,
        bytes calldata payload
    ) external;
}

// Codegen from ABI file to interact with the contract.
// TODO: Remove this from here into an external file
sol!(
  #[allow(missing_docs)]
  #[sol(bytecode = "6080604052348015600e575f80fd5b506107918061001c5f395ff3fe608060405234801561000f575f80fd5b506004361061004a575f3560e01c80631789cd631461004e57806361a93c871461007e578063677087c9146100ae578063837298e9146100de575b5f80fd5b610068600480360381019061006391906103ae565b6100fa565b6040516100759190610423565b60405180910390f35b6100986004803603810190610093919061043c565b610238565b6040516100a5919061047f565b60405180910390f35b6100c860048036038101906100c391906104c2565b610280565b6040516100d59190610423565b60405180910390f35b6100f860048036038101906100f39190610500565b6102e0565b005b5f805f808673ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020015f2090505f818054905090505f468733434244878c8c60405160240161016499989796959493929190610639565b60405160208183030381529060405263837298e960e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f818051906020012090508381908060018154018082558091505060019003905f5260205f20015f9091909190915055828873ffffffffffffffffffffffffffffffffffffffff167fc05d337121a6e8605c6ec0b72aa29c4210ffe6e5b9cefdd6a7058188a8f66f9884604051610222919061070e565b60405180910390a3809450505050509392505050565b5f805f8373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020015f20805490509050919050565b5f805f8473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020015f2082815481106102cf576102ce61072e565b5b905f5260205f200154905092915050565b505050505050505050565b5f80fd5b5f80fd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f61031c826102f3565b9050919050565b61032c81610312565b8114610336575f80fd5b50565b5f8135905061034781610323565b92915050565b5f80fd5b5f80fd5b5f80fd5b5f8083601f84011261036e5761036d61034d565b5b8235905067ffffffffffffffff81111561038b5761038a610351565b5b6020830191508360018202830111156103a7576103a6610355565b5b9250929050565b5f805f604084860312156103c5576103c46102eb565b5b5f6103d286828701610339565b935050602084013567ffffffffffffffff8111156103f3576103f26102ef565b5b6103ff86828701610359565b92509250509250925092565b5f819050919050565b61041d8161040b565b82525050565b5f6020820190506104365f830184610414565b92915050565b5f60208284031215610451576104506102eb565b5b5f61045e84828501610339565b91505092915050565b5f819050919050565b61047981610467565b82525050565b5f6020820190506104925f830184610470565b92915050565b6104a181610467565b81146104ab575f80fd5b50565b5f813590506104bc81610498565b92915050565b5f80604083850312156104d8576104d76102eb565b5b5f6104e585828601610339565b92505060206104f6858286016104ae565b9150509250929050565b5f805f805f805f805f6101008a8c03121561051e5761051d6102eb565b5b5f61052b8c828d016104ae565b995050602061053c8c828d01610339565b985050604061054d8c828d01610339565b975050606061055e8c828d016104ae565b965050608061056f8c828d016104ae565b95505060a06105808c828d016104ae565b94505060c06105918c828d016104ae565b93505060e08a013567ffffffffffffffff8111156105b2576105b16102ef565b5b6105be8c828d01610359565b92509250509295985092959850929598565b6105d981610312565b82525050565b5f82825260208201905092915050565b828183375f83830152505050565b5f601f19601f8301169050919050565b5f61061883856105df565b93506106258385846105ef565b61062e836105fd565b840190509392505050565b5f6101008201905061064d5f83018c610470565b61065a602083018b6105d0565b610667604083018a6105d0565b6106746060830189610470565b6106816080830188610470565b61068e60a0830187610470565b61069b60c0830186610470565b81810360e08301526106ae81848661060d565b90509a9950505050505050505050565b5f81519050919050565b8281835e5f83830152505050565b5f6106e0826106be565b6106ea81856105df565b93506106fa8185602086016106c8565b610703816105fd565b840191505092915050565b5f6020820190508181035f83015261072681846106d6565b905092915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52603260045260245ffdfea2646970667358221220170ea2b6b0dca75d1f0ed969e8703922be925699df71cc2b5f493dbf5af2b09964736f6c634300081a0033")]
  #[sol(rpc)]
  #[derive(Debug)]
  contract InputBox {

    event InputAdded(
      address indexed appContract,
      uint256 indexed index,
      bytes input
    );
    /// @notice Mapping of application contract addresses to arrays of input hashes.
    mapping(address => bytes32[]) private _inputBoxes;

    constructor() {}

    function EvmAdvance(
      uint256 chainId,
      address appContract,
      address msgSender,
      uint256 blockNumber,
      uint256 blockTimestamp,
      uint256 prevRandao,
      uint256 index,
      bytes calldata payload
    ) external;

    function addInput(
      address appContract,
      bytes calldata payload
    ) external returns (bytes32) {
      bytes32[] storage inputBox = _inputBoxes[appContract];
      uint256 index = inputBox.length;
      bytes memory input = abi.encodeCall(
        InputBox.EvmAdvance,
        (
          block.chainid,
          appContract,
          msg.sender,
          block.number,
          block.timestamp,
          block.prevrandao,
          index,
          payload
        )
      );

      bytes32 inputHash = keccak256(input);

      inputBox.push(inputHash);

      emit InputAdded(appContract, index, input);

      return inputHash;
    }

    function getNumberOfInputs(
      address appContract
    ) external view returns (uint256) {
      return _inputBoxes[appContract].length;
    }

    function getInputHash(
      address appContract,
      uint256 index
    ) external view returns (bytes32) {
      return _inputBoxes[appContract][index];
    }
  }
);

#[derive(Deserialize, PartialEq)]
enum DALayer {
    EVM,
    Celestia,
    Avail,
    Espresso,
}

#[derive(Deserialize)]
struct Config {
    base_url: String,
    sequencer_address: Address,
    sequencer_signer_string: String,
    input_box_address: Address,
    da_layer: DALayer,
    auth_token: String,
    namespace: String,
    seed: String,
    app_id: u32,
    vm_id: u32,
}

impl Config {
    fn get_signer(&self) -> Wallet<ecdsa::SigningKey> {
        self.sequencer_signer_string
            .parse::<alloy_signer_wallet::LocalWallet>()
            .expect("Could not parse sequencer signature")
    }
}

struct Lambda {
    wallet_state: WalletState,
    batch_builder: BatchBuilder,
    config: Config,
    provider: Box<dyn Provider<alloy_transport_http::Http<reqwest::Client>>>,
    // used to keep anvil alive during the lifetime of Lambda
    _anvil_instance: Option<AnvilInstance>,
}
impl Lambda {
    // TODO: send the build_batch logic to the specific DA backend
    async fn build_batch(&mut self) -> Result<(), Error> {
        // get the current batch and reset the batch builder
        let batch = self.batch_builder.clone().build();
        self.batch_builder = BatchBuilder::new(self.config.sequencer_address);

        let tx = &batch.clone().to_bytes();

        match self.config.da_layer {
            DALayer::EVM => {
                let signer = self.config.get_signer();

                // get the current batch and reset the batch builder
                let batch = self.batch_builder.clone().build();
                self.batch_builder = BatchBuilder::new(self.config.sequencer_address);

                let provider = ProviderBuilder::new()
                    .with_recommended_fillers()
                    .signer(EthereumSigner::from(signer.clone()))
                    .on_http(self.config.base_url.parse().unwrap());
                // TODO: try to use the lambda's provider instead, but it seems that
                //       it cannot be cloned. or something
                //         let provider = self.provider.clone();

                let input_contract = InputBox::new(self.config.input_box_address, provider);

                // TODO: calculate gas needed
                // TODO: calculate gas price
                let tx = input_contract.addInput(
                    self.config.input_box_address,
                    Bytes::copy_from_slice(&batch.clone().to_bytes()),
                );

                // build event for watching
                let event = input_contract.InputAdded_filter();
                let _ = tx.send().await?.get_receipt().await?;
                // now go listen to the events
                let log = event.query().await.unwrap();
                let event = &log[0].0;

                // testing if the batch is contained in the logs
                // TODO: improve this test to see if it is correctly inserted
                let b: &[u8] = &batch.clone().to_bytes();
                let r = event
                    .input
                    .clone()
                    .windows(b.len())
                    .position(|window| window == b);
                assert!(r.is_some());

                // TODO: the improvement below does not work for some reason
                // let input = event.input.clone();
                // let decoded_advance =
                //     EvmAdvanceCall::abi_decode_raw(&input, true).unwrap();
                // let emitted_batch = decoded_advance.payload;
                // assert_eq!(emitted_batch, batch.to_bytes());

                // TODO: in production someone can break the above assertions
                //       by submitting an input at the same time

                println!("log {:?}", log);
            }
            DALayer::Celestia => {
                let client =
                    celestia_rpc::Client::new(&self.config.base_url, Some(&self.config.auth_token))
                        .await
                        .expect("Failed creating rpc client");

                let data = tx.clone();
                let blob = Blob::new(
                    Namespace::new_v0(&hex::decode(self.config.namespace.clone()).unwrap())
                        .expect("Invalid namespace"),
                    data,
                )
                .unwrap();

                client
                    .blob_submit(&[blob], GasPrice::default())
                    .await
                    .unwrap();
            }
            DALayer::Espresso => {
                let txn = EspressoTransaction::new((self.config.vm_id as u64).into(), tx.clone());

                let client: surf_disco::Client<tide_disco::error::ServerError, SequencerVersion> =
                    surf_disco::Client::new(self.config.base_url.parse().unwrap());
                client
                    .post::<()>("v0/submit/submit")
                    .body_json(&txn)
                    .unwrap()
                    .send()
                    .await
                    .unwrap();
            }
            DALayer::Avail => {
                let client = SDK::new(&self.config.base_url).await.unwrap();
                let secret_uri = SecretUri::from_str(&self.config.seed).unwrap();
                let account = Keypair::from_uri(&secret_uri).unwrap();
                let account_id = account.public_key().to_account_id();

                let nonce = client.api.tx().account_nonce(&account_id).await.unwrap();
                let data = Data { 0: tx.to_vec() };

                let call = avail::tx().data_availability().submit_data(data);
                let params = AvailExtrinsicParamsBuilder::new()
                    .nonce(nonce)
                    .app_id(self.config.app_id)
                    .build();

                let maybe_tx_progress = client
                    .api
                    .tx()
                    .sign_and_submit_then_watch(&call, &account, params)
                    .await;

                client
                    .util
                    .progress_transaction(maybe_tx_progress, WaitFor::BlockInclusion)
                    .await
                    .unwrap();
            }
        }
        Ok(())
    }
}

type LambdaMutex = Mutex<Lambda>;

fn mock_state() -> WalletState {
    let john_address = address!("0000000000000000000000000000000000000099");
    let joe_address = address!("0000000000000000000000000000000000000045");
    let app1_address = address!("0000000000000000000000000000000000000003");
    let app2_address = address!("0000000000000000000000000000000000000023");
    let signer_address = address!("7306897365c277A6951FDA9519fD0CCc16341E4A");
    let mut app1_nonces: AppNonces = AppNonces::new();
    app1_nonces.set_nonce(john_address, 3);
    app1_nonces.set_nonce(joe_address, 15);
    let mut app2_nonces: AppNonces = AppNonces::new();
    app2_nonces.set_nonce(john_address, 22);
    let mut wallet_state: WalletState = WalletState::new();
    wallet_state.add_app_nonce(app1_address, app1_nonces);
    wallet_state.add_app_nonce(app2_address, app2_nonces);
    wallet_state.deposit(john_address, U256::from(2000000000));
    wallet_state.deposit(joe_address, U256::from(321));
    wallet_state.deposit(signer_address, U256::from(2000000000));
    wallet_state
}

#[tokio::main]
async fn main() {
    let config_string = fs::read_to_string("config_default.toml").unwrap();
    let mut config: Config = toml::from_str(&config_string).unwrap();

    // Create a provider with the HTTP transport using the `reqwest` crate.
    let (provider, signer) = if USE_LOCAL_ANVIL {
        let anvil = Anvil::new().try_spawn().expect("Anvil not working");
        let signer: LocalWallet = anvil.keys()[0].clone().into();
        let rpc_url: String = anvil.endpoint().parse().expect("Could not get Anvil's url");
        config.base_url = rpc_url.clone();
        (
            Box::new(
                ProviderBuilder::new()
                    .with_recommended_fillers()
                    .signer(EthereumSigner::from(signer.clone()))
                    .on_http(config.base_url.parse().unwrap()),
            ),
            signer,
        )
    } else {
        let signer = config
            .sequencer_signer_string
            .parse::<alloy_signer_wallet::LocalWallet>()
            .expect("Could not parse sequencer signature");
        (
            Box::new(
                ProviderBuilder::new()
                    .with_recommended_fillers()
                    .signer(EthereumSigner::from(signer.clone()))
                    .on_http(config.base_url.parse().unwrap()),
            ),
            signer,
        )
    };

    if config.da_layer == DALayer::EVM {
        fund_sequencer(
            signer.address(),
            config.sequencer_address,
            Box::new(provider.clone()),
        )
        .await;
        if DEPLOY_INPUT_BOX {
            let nonce = provider
                .get_transaction_count(signer.address())
                .await
                .unwrap();
            config.input_box_address = InputBox::deploy_builder(provider.clone())
                .nonce(nonce)
                .from(signer.address())
                .deploy()
                .await
                .unwrap()
        }
    }

    let wallet_state = mock_state();
    let lambda: LambdaMutex = Mutex::new(Lambda {
        wallet_state,
        batch_builder: BatchBuilder::new(config.sequencer_address),
        config,
        provider,
        _anvil_instance: None,
    });

    let shared_state = Arc::new(lambda);

    let state_copy_for_batches = shared_state.clone();

    // this thread will periodically try to build a batch
    task::spawn(async move {
        loop {
            println!("Building batch...");
            // TODO: investigate why there are no transactions when the batch is empty
            let mut state = state_copy_for_batches.lock().await;
            if state.batch_builder.txs.len() > 0 {
                let _ = state.build_batch().await.unwrap();
            } else {
                println!("Skipping batch, no transactions");
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });

    // initialize tracing
    tracing_subscriber::fmt::init();
    let cors = tower_http::cors::CorsLayer::permissive();
        
    let app = Router::new()
        // `GET /nonce` gets user nonce (see nonce function)
        .route("/nonce", get(get_nonce))
        // `GET /domain` gets the domain
        .route("/domain", get(get_domain))
        // `GET /gas` gets price of gas (see gas function)
        .route("/gas", get(gas_price))
        // `POST /transaction` posts a transaction
        .route("/transaction", post(submit_transaction))
        // `GET /batch` posts a transaction
        .route("/batch", get(get_batch))
        .route("/health", get(health))
        .with_state(shared_state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(":::3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_batch(State(state): State<Arc<LambdaMutex>>) -> (StatusCode, Json<BatchBuilder>) {
    (
        StatusCode::OK,
        Json(state.lock().await.batch_builder.clone()),
    )
}

async fn get_nonce(
    State(state): State<Arc<LambdaMutex>>,
    Json(payload): Json<NonceIdentifier>,
) -> (StatusCode, Json<Nonce>) {
    println!(
        "Getting nonce from user {:?} to application {:?}",
        payload.user, payload.application
    );
    let lambda = state.lock().await;
    let nonce = lambda
        .wallet_state
        .app_nonces
        .get(&payload.application)
        .map(|app_nonces| app_nonces.get_nonce(&payload.user))
        .unwrap_or(Some(&0))
        .unwrap_or(&0);

    let result = Nonce { nonce: *nonce };
    (StatusCode::OK, Json(result))
}

// the input to `nonce` handler
#[derive(Serialize, Deserialize, Debug)]
struct NonceIdentifier {
    user: Address,
    application: Address,
}

// the output of `nonce` handler
#[derive(Serialize)]
struct Nonce {
    nonce: u64,
}

async fn gas_price(
    State(state): State<Arc<LambdaMutex>>,
) -> Result<(StatusCode, Json<u128>), (StatusCode, String)> {
    match get_gas_price(state).await {
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        Ok(gas) => Ok((StatusCode::OK, Json(gas))),
    }
}

async fn get_gas_price(state: Arc<LambdaMutex>) -> Result<u128, Error> {
    Ok(state.lock().await.provider.get_gas_price().await?)
}

async fn get_domain(State(_state): State<Arc<LambdaMutex>>) -> (StatusCode, Json<Eip712Domain>) {
    (StatusCode::OK, Json(DOMAIN))
}

async fn health(State(_state): State<Arc<LambdaMutex>>) -> (StatusCode) {
    (StatusCode::OK)
}

async fn submit_transaction(
    State(state): State<Arc<LambdaMutex>>,
    Json(signed_transaction): Json<SignedTransaction>,
) -> Result<(StatusCode, ()), (StatusCode, String)> {
    if let Err(e) = signed_transaction.recover(&DOMAIN) {
        return Err((StatusCode::UNAUTHORIZED, e.to_string()));
    };
    // TODO: add logic to calculate wei per byte, now it is wei per gas
    // TODO: send the gas logic to the specific DA backend
    let mut state_lock = state.lock().await;
    // TODO: check gas prices on other DA's
    if state_lock.config.da_layer == DALayer::EVM {
        let gas_price = match get_gas_price(state.clone()).await {
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
            Ok(g) => g,
        };
        if signed_transaction.message.max_gas_price < gas_price {
            return Err((
                StatusCode::PAYMENT_REQUIRED,
                format!(
                    "Max gas too small, offered {:}, needed {:}",
                    signed_transaction.message.max_gas_price, gas_price
                )
                .to_string(),
            ));
        }
    }
    let sequencer_address = state_lock.config.sequencer_address.clone();
    let transaction_opt = state_lock
        .wallet_state
        .verify_single(sequencer_address, &signed_transaction.to_wire_transaction());
    state_lock.batch_builder.add(signed_transaction.clone());
    if let None = transaction_opt {
        return Err((
            StatusCode::NOT_ACCEPTABLE,
            "Transaction not valid".to_string(),
        ));
    };
    Ok((StatusCode::CREATED, ()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::SignerSync;
    use axum::{
        body::{Body, Bytes},
        http::{self, Request, StatusCode},
        response::Response,
        routing::RouterIntoService,
    };
    use http_body_util::BodyExt; // for `collect`
    use message::{SignedTransaction, SigningMessage, DOMAIN};
    use mime;
    use serde_json::json;
    use tower::Service;
    use tower::ServiceExt; // for `call`, `oneshot`, and `ready`

    async fn mock_lambda() -> Lambda {
        let config_string = fs::read_to_string("config.toml").unwrap();
        let mut config: Config = toml::from_str(&config_string).unwrap();

        let wallet_state = mock_state();

        let anvil = Anvil::new().try_spawn().expect("Anvil not working");
        if USE_LOCAL_ANVIL {
            let rpc_url: String = anvil.endpoint().parse().expect("Could not get Anvil's url");
            config.base_url = rpc_url.clone();
        }

        let signer: LocalWallet = anvil.keys()[0].clone().into();

        let sequencer_address = config
            .sequencer_signer_string
            .parse::<alloy_signer_wallet::LocalWallet>()
            .expect("Could not parse sequencer signature");

        let provider = ProviderBuilder::new()
            .with_recommended_fillers()
            .signer(EthereumSigner::from(signer.clone()))
            .on_http(config.base_url.clone().parse().unwrap());

        if DEPLOY_INPUT_BOX {
            let nonce = provider
                .get_transaction_count(signer.address())
                .await
                .unwrap();
            config.input_box_address = InputBox::deploy_builder(provider.clone())
                .nonce(nonce)
                .from(signer.address())
                .deploy()
                .await
                .unwrap()
        }

        fund_sequencer(
            signer.address(),
            sequencer_address.address(),
            Box::new(provider.clone()),
        )
        .await;

        let balance = provider
            .get_balance(sequencer_address.address())
            .await
            .unwrap();
        println!(
            "mock_lambda: balance = {:?}, address = {:?}",
            balance,
            sequencer_address.address()
        );

        Lambda {
            wallet_state,
            batch_builder: BatchBuilder::new(config.sequencer_address),
            config,
            provider: Box::new(provider),
            _anvil_instance: Some(anvil),
        }
    }

    fn produce_tx(nonce: u64, gas: u128) -> WireTransaction {
        let json = format!(
            r#"
        {{
            "app":"0x0000000000000000000000000000000000000000",
            "nonce":{nonce},
            "max_gas_price":{gas},
            "data":"0x48656c6c6f2c20576f726c6421"
        }}
        "#
        );
        let v: SigningMessage = serde_json::from_str(&json).unwrap();
        let signer = LocalWallet::random();
        let signature = signer.sign_typed_data_sync(&v, &DOMAIN).unwrap();
        let signed_transaction = SignedTransaction {
            message: v,
            signature,
        };
        WireTransaction::from_signed_transaction(&signed_transaction)
    }

    /// Having a function that produces our app makes it easy to call it from tests
    /// without having to create an HTTP server.
    async fn app() -> (Router, Arc<Mutex<Lambda>>) {
        let lambda = mock_lambda().await;
        let shared_state = Arc::new(Mutex::new(lambda));
        let returned_state = shared_state.clone();
        (
            Router::new()
                .route("/nonce", get(get_nonce))
                .route("/gas", get(gas_price))
                .route("/domain", get(get_domain))
                .route("/transaction", post(submit_transaction))
                .route("/batch", get(get_batch))
                .with_state(shared_state),
            returned_state,
        )
    }

    fn make_request(is_post: bool, uri: &str, body: Body) -> Request<Body> {
        Request::builder()
            .uri(uri)
            .method(if is_post {
                http::Method::POST
            } else {
                http::Method::GET
            })
            .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
            .body(body)
            .unwrap()
    }

    async fn extract_parts(response: Response<Body>) -> (StatusCode, Bytes) {
        let (status, body) = (
            response.status(),
            response.into_body().collect().await.unwrap().to_bytes(),
        );
        println!("status: {:}, body: {:?}", status, &body);
        (status, body)
    }

    #[tokio::test]
    async fn gas() {
        let (app, _) = app().await;
        let mut service: RouterIntoService<Body> = app.into_service();
        let response = ServiceExt::<Request<Body>>::ready(&mut service)
            .await
            .unwrap()
            .call(make_request(false, "/gas", Body::empty()))
            .await
            .unwrap();
        let (status, _body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn domain() {
        let (app, _) = app().await;
        let response = app
            .oneshot(make_request(false, "/domain", Body::empty()))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[..], b"{\"name\":\"CartesiPaio\",\"version\":\"0.0.1\",\"chainId\":\"0x7A69\",\"verifyingContract\":\"0x0000000000000000000000000000000000000000\"}");
    }

    #[tokio::test]
    async fn transaction_low_gas() {
        let (app, _) = app().await;
        let transaction = produce_tx(21, 21).to_signed_transaction();
        let response = app
            .oneshot(make_request(
                true,
                "/transaction",
                Body::from(serde_json::to_vec(&json!(transaction)).unwrap()),
            ))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
        assert_eq!(&body[0..37], b"Max gas too small, offered 21, needed");
    }

    #[tokio::test]
    async fn transaction_low_balance() {
        let (app, _) = app().await;
        let transaction = produce_tx(21, 2000000000).to_signed_transaction();
        let response = app
            .oneshot(make_request(
                true,
                "/transaction",
                Body::from(serde_json::to_vec(&json!(transaction)).unwrap()),
            ))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::NOT_ACCEPTABLE);
        assert_eq!(&body[..], b"Transaction not valid");
    }

    #[tokio::test]
    async fn transaction_success() {
        let (app, _) = app().await;
        let transaction = produce_tx(0, 2000000000).to_signed_transaction();
        let response = app
            .oneshot(make_request(
                true,
                "/transaction",
                Body::from(serde_json::to_vec(&json!(transaction)).unwrap()),
            ))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(&body[..], b"");
    }

    #[tokio::test]
    async fn batch_filling() {
        let (app, state) = app().await;
        let mut service: RouterIntoService<Body> = app.into_service();
        let response = ServiceExt::<Request<Body>>::ready(&mut service)
            .await
            .unwrap()
            .call(make_request(false, "/batch", Body::empty()))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[..], b"{\"sequencer_payment_address\":\"0x63f9725f107358c9115bc9d86c72dd5823e9b1e6\",\"txs\":[]}");
        let transaction = produce_tx(0, 2000000000).to_signed_transaction();
        let response = ServiceExt::<Request<Body>>::ready(&mut service)
            .await
            .unwrap()
            .call(make_request(
                true,
                "/transaction",
                Body::from(serde_json::to_vec(&json!(transaction)).unwrap()),
            ))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(&body[..], b"");
        let response = ServiceExt::<Request<Body>>::ready(&mut service)
            .await
            .unwrap()
            .call(make_request(false, "/batch", Body::empty()))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        // here we ommit the signature and only look at the first bytes,
        // because the signature changes every time.
        assert_eq!(&body[0..169], b"{\"sequencer_payment_address\":\"0x63f9725f107358c9115bc9d86c72dd5823e9b1e6\",\"txs\":[{\"message\":{\"app\":\"0x0000000000000000000000000000000000000000\",\"nonce\":0,\"max_gas_price\"");
        let mut state_lock = state.lock().await;
        let _batch = state_lock.build_batch().await.unwrap();

        let provider = ProviderBuilder::new().on_http(state_lock.config.base_url.parse().unwrap());

        let input_contract = InputBox::new(state_lock.config.input_box_address, provider.clone());

        let hash = input_contract
            .getInputHash(state_lock.config.input_box_address, U256::from(0))
            .call()
            .await
            .unwrap();

        println!("hash = {:}", hash._0);

        // TODO: test if batch was submitted to inputbox
    }

    #[tokio::test]
    async fn nonce_miss() {
        let (app, _) = app().await;
        let nonce_id = NonceIdentifier {
            application: address!("0000000000000000000000000000000000000010"),
            user: address!("0000000000000000000000000000000000000020"),
        };
        let response = app
            .oneshot(make_request(
                false,
                "/nonce",
                Body::from(serde_json::to_vec(&json!(nonce_id)).unwrap()),
            ))
            .await
            .unwrap();
        println!("{:?}", response);
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[..], b"{\"nonce\":0}");
    }

    #[tokio::test]
    async fn nonce() {
        let (app, _) = app().await;
        let nonce_id = NonceIdentifier {
            user: address!("0000000000000000000000000000000000000099"),
            application: address!("0000000000000000000000000000000000000003"),
        };
        println!("{:?}", nonce_id);
        let response = app
            .oneshot(make_request(
                false,
                "/nonce",
                Body::from(serde_json::to_vec(&json!(nonce_id)).unwrap()),
            ))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[..], b"{\"nonce\":3}");
    }
}
