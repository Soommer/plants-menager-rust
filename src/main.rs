use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json,
    Router,
};

use chrono::{NaiveDate, NaiveDateTime};

use serde::Serialize;
use sqlx::{PgPool, FromRow};
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

#[derive(Serialize, FromRow)]
struct PlantResponse {
    id: i32,
    name: String,
    latin_name: Option<String>,
    family_id: i32,
    date_of_last_watering: Option<NaiveDateTime>,
    date_of_aquirement: Option<NaiveDate>,
    notes: Option<String>,
}

#[tokio::main]
async fn main(){
    // wczytywanie env
    dotenvy::dotenv().ok();

    // db connection string
    let database_url = std::env::var("DATABASE_URL")
        .expect("Brak zmiennej DATABASE_URL w .env");

    let db = sqlx::PgPool::connect(&database_url)
        .await
        .expect("NIe udało się połączyć z bazą");

    let state = AppState {db};
    // Router
    let app = Router::new()
        .route("/api/health/live", get(health_check))
        .route("/api/health/ready", get(health_response))
        .route("/api/plants", get(get_plants))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server listining on :3000");

    axum::serve(listener,app).await.unwrap();
}

// Sprawdzenie komunikacji z api (ping)
async fn health_check() -> StatusCode{
    StatusCode::OK
}


async fn health_response(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<HealthResponse>)> {
    // Inicjacja HealthResponse
    let mut healthresponse:HealthResponse = HealthResponse { status: ("healthy".to_string()), database: ("".to_string()), version: env!("CARGO_PKG_VERSION") };
    let mut is_healthy: bool = true; // health flag
    // Testy połączenia i statusu db
    let db_check = sqlx::query("SELECT 1")
        .execute(&state.db)
        .await;
    match db_check{
        Ok(_)=> {
            healthresponse.database = "up".to_string();
        }
        Err(e)=> {
            eprintln!("Bład połączenia z bazą danych: {}", e);
            healthresponse.database = "down".to_string();
            healthresponse.status = "bad".to_string();
            is_healthy = false;
        }
    }
    if is_healthy {
        Ok(Json(healthresponse))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(healthresponse)))
    }
}

async fn get_plants(
    State(state): State<AppState>,
) -> Result<Json<Vec<PlantResponse>>, (StatusCode, String)>{
    // Wykonanie zapytania 
    let plants = sqlx::query_as::<_, PlantResponse>(
        "SELECT id, name, latin_name, family_id, date_of_last_watering, date_of_aquirement, notes FROM plant"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e|{
        // Logowanie błędu
        eprintln!("Błąd podczas pobierania roślin z bazy: {}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Błąd bazy danych: {}", e),
    )
    })?;
    // Zwrot jeśli ok
    Ok(Json(plants))
}