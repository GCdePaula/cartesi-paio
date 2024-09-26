use alloy_core::{
    primitives::{address, Address, Bytes, U256},
    sol,
    sol_types::Eip712Domain,
};
use alloy_network::EthereumSigner;
use alloy_node_bindings::Anvil;
use alloy_node_bindings::AnvilInstance;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_signer::k256::ecdsa;
use alloy_signer_wallet::LocalWallet;
use alloy_signer_wallet::Wallet;
use anyhow::Error;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, options, post},
    Json, Router,
};
use message::WireTransaction;
use message::{AppNonces, BatchBuilder, WalletState, DOMAIN};
use reqwest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;
use toml;
use tower_http::cors::CorsLayer;
use utils::InputBox;

mod utils;

const USE_LOCAL_ANVIL: bool = false;
const DEPLOY_INPUT_BOX: bool = true;

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

#[derive(Deserialize)]
struct Config {
    base_url: String,
    sequencer_address: Address,
    sequencer_signer_string: String,
    input_box_address: Address,
    // TODO: add domain (see in message/lib)
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

        let input_contract =
            InputBox::new(self.config.input_box_address, provider);

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

        // println!("log {:?}", log);

        // TODO: do more error handling
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
    let config_string = fs::read_to_string("config.toml").unwrap();
    let mut config: Config = toml::from_str(&config_string).unwrap();

    // Create a provider with the HTTP transport using the `reqwest` crate.
    let (provider, signer) = if USE_LOCAL_ANVIL {
        let anvil = Anvil::new().try_spawn().expect("Anvil not working");
        let signer: LocalWallet = anvil.keys()[0].clone().into();
        let rpc_url: String =
            anvil.endpoint().parse().expect("Could not get Anvil's url");
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

    // TODO: should detect the need from config
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

    // TODO: load serialized mock_state from file
    let wallet_state = mock_state();
    let lambda: LambdaMutex = Mutex::new(Lambda {
        wallet_state,
        // TODO: Should we also load serialized batch builder from file?
        batch_builder: BatchBuilder::new(config.sequencer_address),
        config,
        provider,
        _anvil_instance: None,
    });

    let shared_state = Arc::new(lambda);

    let state_copy_for_batches = shared_state.clone();

    // TODO: investigate why there are so many frequent eth_blockNumber requests to L1

    // this thread will periodically try to build a batch
    task::spawn(async move {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            print!("Building batch...");
            // TODO: investigate why there are no transactions when the batch is empty
            let mut state = state_copy_for_batches.lock().await;
            let _ = state.build_batch().await.unwrap();
            println!("done.");
        }
    });

    // initialize tracing
    tracing_subscriber::fmt::init();

    let app = Router::new()
        // `GET /nonce` gets user nonce (see nonce function)
        .route("/nonce", post(get_nonce))
        // `GET /domain` gets the domain
        .route("/domain", get(get_domain))
        // `GET /gas` gets price of gas (see gas function)
        .route("/gas", get(gas_price))
        // `POST /transaction` posts a transaction
        .route("/transaction", post(submit_transaction))
        // `GET /batch` posts a transaction
        .route("/batch", get(get_batch))
        // TODO: Think about CORS in production
        .layer(CorsLayer::permissive())
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_batch(
    State(state): State<Arc<LambdaMutex>>,
) -> (StatusCode, Json<BatchBuilder>) {
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

async fn get_domain(
    State(_state): State<Arc<LambdaMutex>>,
) -> (StatusCode, Json<Eip712Domain>) {
    println!("Domain requested: {:?}", Json(DOMAIN));
    (StatusCode::OK, Json(DOMAIN))
}

async fn submit_transaction(
    State(state): State<Arc<LambdaMutex>>,
    Json(payload): Json<WireTransaction>,
) -> Result<(StatusCode, ()), (StatusCode, String)> {
    let signed_transaction = &payload.to_signed_transaction();
    if let Err(e) = signed_transaction.recover(&DOMAIN) {
        return Err((StatusCode::UNAUTHORIZED, e.to_string()));
    };
    // TODO: add logic to calculate wei per byte, now it is wei per gas
    // TODO: send the gas logic to the specific DA backend
    let gas_price = match get_gas_price(state.clone()).await {
        Err(e) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
        Ok(g) => g,
    };
    if payload.max_gas_price < gas_price {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "Max gas too small, offered {:}, needed {:}",
                payload.max_gas_price, gas_price
            )
            .to_string(),
        ));
    }
    let mut state_lock = state.lock().await;
    let sequencer_address = state_lock.config.sequencer_address.clone();
    let transaction_opt = state_lock
        .wallet_state
        .verify_single(sequencer_address, &payload);
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
    use utils::fund_sequencer;

    async fn mock_lambda() -> Lambda {
        let config_string = fs::read_to_string("config.toml").unwrap();
        let mut config: Config = toml::from_str(&config_string).unwrap();

        let wallet_state = mock_state();

        let anvil = Anvil::new().try_spawn().expect("Anvil not working");
        if USE_LOCAL_ANVIL {
            let rpc_url: String =
                anvil.endpoint().parse().expect("Could not get Anvil's url");
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
            config.input_box_address =
                InputBox::deploy_builder(provider.clone())
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
        assert_eq!(&body[..], b"{\"name\":\"CartesiPaio\",\"version\":\"0.0.1\",\"chainId\":\"0x539\",\"verifyingContract\":\"0x0000000000000000000000000000000000000000\"}");
    }

    #[tokio::test]
    async fn transaction_low_gas() {
        let (app, _) = app().await;
        let transaction = produce_tx(21, 21);
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
        let transaction = produce_tx(21, 2000000000);
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
        let transaction = produce_tx(0, 2000000000);
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
        let transaction = produce_tx(0, 2000000000);
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

        let provider = ProviderBuilder::new()
            .on_http(state_lock.config.base_url.parse().unwrap());

        let input_contract = InputBox::new(
            state_lock.config.input_box_address,
            provider.clone(),
        );

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
