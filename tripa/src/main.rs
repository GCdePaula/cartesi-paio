use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use mime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

type Address = u8; // placeholder until we add the real thing
type U256 = u16; // placeholder until we add the real thing

struct Lambda {
    wallet_state: WalletState,
    // TODO: add mem_pool,
}

struct WalletState {
    wallets: HashMap<Address, Wallet>,
}

struct Wallet {
    nonce: HashMap<Address, u64>,
    balance: U256,
}

fn mock_lambda() -> Lambda {
    let mut nonces_john = HashMap::new();
    nonces_john.insert(3, 2);
    nonces_john.insert(23, 22);
    let mut john = Wallet {
        nonce: nonces_john,
        balance: 234,
    };
    let mut nonces_joe = HashMap::new();
    nonces_joe.insert(1, 92);
    nonces_joe.insert(22, 111);
    let mut joe = Wallet {
        nonce: nonces_joe,
        balance: 98,
    };
    let mut wallets = HashMap::new();
    wallets.insert(99, john);
    wallets.insert(45, joe);
    let wallet_state = WalletState { wallets };
    Lambda { wallet_state }
}

#[tokio::main]
async fn main() {
    let mut lambda = mock_lambda();
    let shared_state = Arc::new(lambda);

    // initialize tracing
    tracing_subscriber::fmt::init();

    // TODO: get everything necessary for EIP 712's domain
    // TODO: add an endpoint to get the DOMAIN
    let app = Router::new()
        // `GET /nonce` gets user nonce (see nonce function)
        .route("/nonce", get(nonce))
        // `GET /gas` gets price of gas (see gas function)
        .route("/gas", get(gas_price))
        // `POST /transaction` posts a transaction
        .route("/transaction", post(submit_transaction))
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn nonce(
    State(state): State<Arc<Lambda>>,
    Json(payload): Json<NonceIdentifier>,
) -> (StatusCode, Json<Nonce>) {
    println!(
        "Getting nonce from user {:?} to application {:?}",
        payload.user, payload.application
    );
    let null_wallet = Wallet {
        nonce: HashMap::new(),
        balance: 0,
    };
    let user_wallet = state
        .wallet_state
        .wallets
        .get(&payload.user)
        .unwrap_or(&null_wallet);
    let nonce = user_wallet.nonce.get(&payload.application).unwrap_or(&0);

    let result = Nonce { nonce: *nonce };
    (StatusCode::OK, Json(result))
}

// the input to `nonce` handler
#[derive(Serialize, Deserialize, Debug)]
struct NonceIdentifier {
    application: Address,
    user: Address,
}

// the output of `nonce` handler
#[derive(Serialize)]
struct Nonce {
    nonce: u64,
}

async fn gas_price(
    State(state): State<Arc<Lambda>>,
) -> (StatusCode, Json<Gas>) {
    // TODO: add logic to get gas price
    let gas = Gas { gas_price: 22 };
    (StatusCode::OK, Json(gas))
}

// the output of `gas` handler
#[derive(Serialize)]
struct Gas {
    gas_price: u64,
}

async fn submit_transaction(
    State(state): State<Arc<Lambda>>,
    Json(payload): Json<SignedTransaction>,
) -> Result<(), StatusCode> {
    println!("Received transaction with temperos {:?}", payload.temperos);

    if payload.temperos > 0 {
        // this will be converted into a status code `200 OK`
        // TODO: convert this into the status code `201 Created`
        Ok(())
    } else {
        Err(StatusCode::PAYMENT_REQUIRED)
    }
}

// the input to `submit_transaction` handler
#[derive(Serialize, Deserialize, Debug)]
struct SignedTransaction {
    temperos: i16,
}

/// Having a function that produces our app makes it easy to call it from tests
/// without having to create an HTTP server.
fn app() -> Router {
    let mut lambda = mock_lambda();
    let shared_state = Arc::new(lambda);

    Router::new()
        .route("/nonce", get(nonce))
        .route("/gas", get(gas_price))
        .route("/transaction", post(submit_transaction))
        .with_state(shared_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::connect_info::MockConnectInfo,
        http::{self, Request, StatusCode},
    };
    use http_body_util::BodyExt; // for `collect`
    use serde_json::{json, Value};
    use tokio::net::TcpListener;
    use tower::{Service, ServiceExt}; // for `call`, `oneshot`, and `ready`

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
        assert_eq!(&body[..], b"{\"gas_price\":22}");
    }

    #[tokio::test]
    async fn transaction() {
        let app = app();
        let transaction = SignedTransaction { temperos: 20 };
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
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"");
    }

    #[tokio::test]
    async fn transaction_failed() {
        let app = app();
        let transaction = SignedTransaction { temperos: -2 };
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
        assert_eq!(&body[..], b"");
    }

    #[tokio::test]
    async fn nonce() {
        let app = app();
        let nonce_id = NonceIdentifier {
            application: 10,
            user: 20,
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
    async fn nonce_miss() {
        let app = app();
        let nonce_id = NonceIdentifier {
            user: 99,
            application: 3,
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
        assert_eq!(&body[..], b"{\"nonce\":2}");
    }
}
