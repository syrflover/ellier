use std::{fmt::Debug, str::FromStr};

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;

#[tokio::main]
async fn main() {
    let ctx = Context {
        chzzk_auth: ChzzkAuth {
            nid_ses: env("NID_SES"),
            nid_aut: env("NID_AUT"),
            nid_jkl: env("NID_JKL"),
        },
    };

    let app = Router::new()
        .route("/", get(root))
        .route("/chzzk-auth", get(get_chzzk_auth))
        .with_state(ctx);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn get_chzzk_auth(
    // this argument tells axum to parse the request body
    // as JSON into a `CreateUser` type
    State(ctx): State<Context>,
) -> Json<ChzzkAuth> {
    Json(ctx.chzzk_auth)
}

#[derive(Clone)]
struct Context {
    chzzk_auth: ChzzkAuth,
}

#[derive(Clone, Serialize)]
struct ChzzkAuth {
    nid_ses: String,
    nid_aut: String,
    nid_jkl: String,
}

fn env<T>(key: &str) -> T
where
    T: FromStr,
    <T as FromStr>::Err: Debug,
{
    let var = match std::env::var(key) {
        Ok(r) => r,
        Err(_) => panic!("not set {key}"),
    };

    var.parse().expect("Please set dotenv to valid value")
}
