use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json,
    Router,
};
use serde::Serialize;
use sqlx::PgPool;
use dotenvy;

#[derive(Clone)]
struct AppState {
    db: PgPool,
}


#[derive(Serialize)]
struct HealthResponse {
    status: String,
    database: String,
    version: &'static str,
}

#[tokio::main]
async fn main(){
    // wczytywanie env
    dotenvy::dotenv().ok();

    // db connection string
    let database_url = std::env::var("DATABAES_URL")
        .expect("Brak zmiennej DATABASE_URL w .env");

    let db = sqlx::PgPool::connect(&database_url)
        .await
        .expect("NIe udało się połączyć z bazą");

    let state = AppState {db};
    let app = Router::new()
        .route("/api/health/live", get(health_check))
        .route("/api/health/ready", get(health_response))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server listining on :3000");

    axum::serve(listener,app).await.unwrap();
}

// Sprawdzenie komunikacji z api (ping)
async fn health_check() -> StatusCode{
    StatusCode::OK
}


// Sprawdzenie zależności i gotowości api
async fn health_response(
    State(_state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    // Inicjacja HealthResponse
    let mut healthresponse:HealthResponse = HealthResponse { status: ("healthy".to_string()), database: ("".to_string()), version: env!("CARGO_PKG_VERSION") };
    let mut is_healthy: bool = true; // health flag
    // Testy połączenia i statusu db
    let db_ok:bool = true; // TBA
    if db_ok {
        healthresponse.database = "up".to_string();
    } else {
        healthresponse.database = "down".to_string();
        healthresponse.status = "bad".to_string();
        is_healthy = false;
    };

    if is_healthy {
        Ok(Json(healthresponse))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(healthresponse)))
    }
}
