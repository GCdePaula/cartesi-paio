use alloy_core::primitives::{address, Address, U256};
use alloy_signer::SignerSync;
use alloy_signer_wallet::LocalWallet;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use message::{SignedTransaction, SigningMessage, DOMAIN};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

fn produce_tx() -> SignedTransaction {
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
    SignedTransaction {
        message: v,
        signature,
    }
}

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
    nonces_john.insert(address!("0000000000000000000000000000000000000003"), 2);
    nonces_john
        .insert(address!("0000000000000000000000000000000000000023"), 22);
    let john = Wallet {
        nonce: nonces_john,
        balance: U256::from(234),
    };
    let mut nonces_joe = HashMap::new();
    nonces_joe.insert(address!("0000000000000000000000000000000000000001"), 92);
    nonces_joe
        .insert(address!("0000000000000000000000000000000000000022"), 111);
    let joe = Wallet {
        nonce: nonces_joe,
        balance: U256::from(98),
    };
    let mut wallets = HashMap::new();
    wallets.insert(address!("0000000000000000000000000000000000000099"), john);
    wallets.insert(address!("0000000000000000000000000000000000000045"), joe);
    let wallet_state = WalletState { wallets };
    Lambda { wallet_state }
}

#[tokio::main]
async fn main() {
    let lambda = mock_lambda();
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
        balance: U256::from(0),
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
    State(_state): State<Arc<Lambda>>,
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
    State(_state): State<Arc<Lambda>>,
    Json(payload): Json<SignedTransaction>,
) -> Result<(), StatusCode> {
    //println!("Received transaction with temperos {:?}", payload.temperos);

    //if payload.temperos > 0 {
    // this will be converted into a status code `200 OK`
    // TODO: convert this into the status code `201 Created`
    Ok(())
    //} else {
    //    Err(StatusCode::PAYMENT_REQUIRED)
    //}
}

/// Having a function that produces our app makes it easy to call it from tests
/// without having to create an HTTP server.
fn app() -> Router {
    let lambda = mock_lambda();
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
        http::{self, Request, StatusCode},
    };
    use http_body_util::BodyExt; // for `collect`
    use mime;
    use serde_json::json;
    use tower::ServiceExt; // for `call`, `oneshot`, and `ready`

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
        // let signing_transaction = SigningTransaction {
        //     app: address!("0000000000000000000000000000000000000003"),
        //     nonce: 12,
        //     max_gas_price: 22,
        //     data: vec![],
        // };
        let transaction = produce_tx();
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

    // #[tokio::test]
    // async fn transaction_failed() {
    //     let app = app();
    //     let transaction = SignedTransaction { temperos: -2 };
    //     let response = app
    //         .oneshot(
    //             Request::builder()
    //                 .uri("/transaction")
    //                 .method(http::Method::POST)
    //                 .header(
    //                     http::header::CONTENT_TYPE,
    //                     mime::APPLICATION_JSON.as_ref(),
    //                 )
    //                 .body(Body::from(
    //                     serde_json::to_vec(&json!(transaction)).unwrap(),
    //                 ))
    //                 .unwrap(),
    //         )
    //         .await
    //         .unwrap();
    //     assert_eq!(response.status(), StatusCode::PAYMENT_REQUIRED);
    //     let body = response.into_body().collect().await.unwrap().to_bytes();
    //     assert_eq!(&body[..], b"");
    // }

    #[tokio::test]
    async fn nonce() {
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
    async fn nonce_miss() {
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
        assert_eq!(&body[..], b"{\"nonce\":2}");
    }
}
