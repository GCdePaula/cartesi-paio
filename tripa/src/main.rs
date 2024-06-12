use alloy_core::{
    primitives::{address, Address, U256},
    sol_types::Eip712Domain,
};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use message::WireTransaction;
use message::{AppNonces, BatchBuilder, WalletState, DOMAIN};
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;
use toml;

const SEQUENCER_ADDRESS: Address =
    address!("0000000000000000000000000000022222222222");

struct Lambda {
    wallet_state: WalletState,
    batch_builder: BatchBuilder,
    config: Config,
}

#[derive(Deserialize)]
struct Config {
    base_url: String,
}

type LambdaMutex = Mutex<Lambda>;

fn mock_lambda() -> Lambda {
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
    wallet_state.deposit(john_address, U256::from(123));
    wallet_state.deposit(joe_address, U256::from(321));
    wallet_state.deposit(signer_address, U256::from(30000000));

    let config_string = fs::read_to_string("config.toml").unwrap();
    let config: Config = toml::from_str(&config_string).unwrap();
    Lambda {
        wallet_state,
        batch_builder: BatchBuilder::new(SEQUENCER_ADDRESS),
        config,
    }
}

#[tokio::main]
async fn main() {
    let lambda: LambdaMutex = Mutex::new(mock_lambda());
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
) -> (StatusCode, Json<GasPrice>) {
    // TODO: add logic to get gas price
    let gas: GasPrice = get_gas_price(state).await;
    (StatusCode::OK, Json(gas))
}

async fn get_domain(
    State(_state): State<Arc<LambdaMutex>>,
) -> (StatusCode, Json<Eip712Domain>) {
    // TODO: add logic to get gas price
    (StatusCode::OK, Json(DOMAIN))
}

async fn get_gas_price(_state: Arc<LambdaMutex>) -> GasPrice {
    22
}

// the output of `gas` handler
type GasPrice = u64;

async fn submit_transaction(
    State(state): State<Arc<LambdaMutex>>,
    Json(payload): Json<WireTransaction>,
) -> Result<(StatusCode, ()), (StatusCode, String)> {
    let signed_transaction = &payload.to_signed_transaction();
    if let Err(_) = signed_transaction.recover(&DOMAIN) {
        return Err((StatusCode::UNAUTHORIZED, "Signature error".to_string()));
    };

    if payload.max_gas_price < get_gas_price(state.clone()).await {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "Max gas too small".to_string(),
        ));
    }

    let transaction_opt = state
        .lock()
        .await
        .wallet_state
        .verify_single(SEQUENCER_ADDRESS, &payload);

    state
        .lock()
        .await
        .batch_builder
        .add(signed_transaction.clone());

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
        http::{self, Request, StatusCode},
    };
    use http_body_util::BodyExt; // for `collect`
    use message::{SignedTransaction, SigningMessage, DOMAIN};
    use mime;
    use serde_json::json;
    use tower::ServiceExt; // for `call`, `oneshot`, and `ready`

    fn produce_tx(nonce: u64, gas: u64) -> WireTransaction {
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

    #[tokio::test]
    async fn gas() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder().uri("/gas").body(Body::empty()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"22");
    }

    #[tokio::test]
    async fn domain() {
        let app = app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/domain")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        assert_eq!(&body[..], b"{\"name\":\"CartesiPaio\",\"version\":\"0.0.1\",\"chainId\":\"0x539\",\"verifyingContract\":\"0x0000000000000000000000000000000000000000\"}");
    }

    #[tokio::test]
    async fn transaction_low_gas() {
        let app = app();
        let transaction = produce_tx(21, 21);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/transaction")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        mime::APPLICATION_JSON.as_ref(),
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!(transaction)).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        assert_eq!(&body[..], b"Max gas too small");
    }

    #[tokio::test]
    async fn transaction_low_balance() {
        let app = app();
        let transaction = produce_tx(21, 210);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/transaction")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        mime::APPLICATION_JSON.as_ref(),
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!(transaction)).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        assert_eq!(&body[..], b"Transaction not valid");
    }

    #[tokio::test]
    async fn transaction_success() {
        let app = app();
        let transaction = produce_tx(0, 22);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/transaction")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        mime::APPLICATION_JSON.as_ref(),
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!(transaction)).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        assert_eq!(&body[..], b"");
    }

    #[tokio::test]
    async fn batch_filling() {
        let app = app();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/batch")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        assert_eq!(&body[..], b"{\"sequencer_payment_address\":\"0x0000000000000000000000000000022222222222\",\"txs\":[]}");
        let transaction = produce_tx(0, 22);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/transaction")
                    .method(http::Method::POST)
                    .header(
                        http::header::CONTENT_TYPE,
                        mime::APPLICATION_JSON.as_ref(),
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!(transaction)).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        assert_eq!(&body[..], b"");
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/batch")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", &body);
        // here we ommit the signature and only look at the first bytes,
        // because the signature changes every time.
        assert_eq!(&body[0..230], b"{\"sequencer_payment_address\":\"0x0000000000000000000000000000022222222222\",\"txs\":[{\"message\":{\"app\":\"0x0000000000000000000000000000000000000000\",\"nonce\":0,\"max_gas_price\":22,\"data\":\"0x48656c6c6f2c20576f726c6421\"},\"signature\":{\"r\":\"");
    }

    #[tokio::test]
    async fn nonce_miss() {
        let app = app();
        let nonce_id = NonceIdentifier {
            application: address!("0000000000000000000000000000000000000010"),
            user: address!("0000000000000000000000000000000000000020"),
        };
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/nonce")
                    .method(http::Method::GET)
                    .header(
                        http::header::CONTENT_TYPE,
                        mime::APPLICATION_JSON.as_ref(),
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!(nonce_id)).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        println!("{:?}", response);
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        println!("{:?}", std::str::from_utf8(&body[..]).unwrap());
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
            .oneshot(
                Request::builder()
                    .uri("/nonce")
                    .method(http::Method::GET)
                    .header(
                        http::header::CONTENT_TYPE,
                        mime::APPLICATION_JSON.as_ref(),
                    )
                    .body(Body::from(
                        serde_json::to_vec(&json!(nonce_id)).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"{\"nonce\":3}");
    }
}
