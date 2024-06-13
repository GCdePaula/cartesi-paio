use alloy_core::{
    primitives::{address, Address, U256},
    sol_types::Eip712Domain,
};
use alloy_node_bindings::AnvilInstance;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_transport_http;
use anyhow::Error;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use message::WireTransaction;
use message::{AppNonces, BatchBuilder, WalletState, DOMAIN};
use reqwest;
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use toml;

struct Lambda {
    wallet_state: WalletState,
    batch_builder: BatchBuilder,
    config: Config,
    provider: Box<dyn Provider<alloy_transport_http::Http<reqwest::Client>>>,
    _anvil_instance: Option<AnvilInstance>,
}

#[derive(Deserialize)]
struct Config {
    base_url: String,
    sequencer_address: Address,
    // TODO: add domain (see in message/lib)
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
    let config: Config = toml::from_str(&config_string).unwrap();

    // Create a provider with the HTTP transport using the `reqwest` crate.
    let provider =
        ProviderBuilder::new().on_http(config.base_url.parse().unwrap());

    let wallet_state = mock_state();
    let lambda: LambdaMutex = Mutex::new(Lambda {
        wallet_state,
        batch_builder: BatchBuilder::new(config.sequencer_address),
        config,
        provider: Box::new(provider),
        _anvil_instance: None,
    });

    let shared_state = Arc::new(lambda);

    // initialize tracing
    tracing_subscriber::fmt::init();

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
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
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
) -> Result<(StatusCode, Json<GasPrice>), (StatusCode, String)> {
    match get_gas_price(state).await {
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
        Ok(gas) => Ok((StatusCode::OK, Json(gas))),
    }
}

async fn get_gas_price(state: Arc<LambdaMutex>) -> Result<GasPrice, Error> {
    Ok(state.lock().await.provider.get_gas_price().await?)
}

async fn get_domain(
    State(_state): State<Arc<LambdaMutex>>,
) -> (StatusCode, Json<Eip712Domain>) {
    (StatusCode::OK, Json(DOMAIN))
}

// the output of `gas` handler
type GasPrice = u128;

async fn submit_transaction(
    State(state): State<Arc<LambdaMutex>>,
    Json(payload): Json<WireTransaction>,
) -> Result<(StatusCode, ()), (StatusCode, String)> {
    let signed_transaction = &payload.to_signed_transaction();
    if let Err(e) = signed_transaction.recover(&DOMAIN) {
        return Err((StatusCode::UNAUTHORIZED, e.to_string()));
    };
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
    use alloy_signer_wallet::LocalWallet;
    use axum::{
        body::Body,
        body::Bytes,
        http::{self, Request, StatusCode},
        response::Response,
    };
    use http_body_util::BodyExt; // for `collect`
    use message::{SignedTransaction, SigningMessage, DOMAIN};
    use mime;
    use serde_json::json;
    use tower::ServiceExt; // for `call`, `oneshot`, and `ready`

    fn mock_lambda() -> Lambda {
        let config_string = fs::read_to_string("config.toml").unwrap();
        let config: Config = toml::from_str(&config_string).unwrap();

        let wallet_state = mock_state();

        let anvil = Anvil::new().try_spawn().expect("Anvil not working");
        let rpc_url: String =
            anvil.endpoint().parse().expect("Could not get Anvil's url");
        Lambda {
            wallet_state,
            batch_builder: BatchBuilder::new(config.sequencer_address),
            config,
            provider: Box::new(
                ProviderBuilder::new()
                    .on_http(rpc_url.clone().parse().unwrap()),
            ),
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
    fn app() -> Router {
        let lambda = Mutex::new(mock_lambda());
        let shared_state = Arc::new(lambda);
        Router::new()
            .route("/nonce", get(get_nonce))
            .route("/gas", get(gas_price))
            .route("/domain", get(get_domain))
            .route("/transaction", post(submit_transaction))
            .route("/batch", get(get_batch))
            .with_state(shared_state)
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
        let app = app();
        let response = app
            .oneshot(make_request(false, "/gas", Body::empty()))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[..], b"2000000000");
    }

    #[tokio::test]
    async fn domain() {
        let app = app();
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
        let app = app();
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
        assert_eq!(
            &body[..],
            b"Max gas too small, offered 21, needed 2000000000"
        );
    }

    #[tokio::test]
    async fn transaction_low_balance() {
        let app = app();
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
        let app = app();
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
        let app = app();
        let response = app
            .clone()
            .oneshot(make_request(false, "/batch", Body::empty()))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(&body[..], b"{\"sequencer_payment_address\":\"0x0000000000000000000000000000022222222222\",\"txs\":[]}");
        let transaction = produce_tx(0, 2000000000);
        let response = app
            .clone()
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
        let response = app
            .oneshot(make_request(false, "/batch", Body::empty()))
            .await
            .unwrap();
        let (status, body) = extract_parts(response).await;
        assert_eq!(status, StatusCode::OK);
        // here we ommit the signature and only look at the first bytes,
        // because the signature changes every time.
        assert_eq!(&body[0..169], b"{\"sequencer_payment_address\":\"0x0000000000000000000000000000022222222222\",\"txs\":[{\"message\":{\"app\":\"0x0000000000000000000000000000000000000000\",\"nonce\":0,\"max_gas_price\"");
    }

    #[tokio::test]
    async fn nonce_miss() {
        let app = app();
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
        let app = app();
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
