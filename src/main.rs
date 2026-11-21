use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json,
    Router,
};

use chrono::{NaiveDate, NaiveDateTime};

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, FromRow};
use dotenvy;

// Logi 
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

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
    family_name: String,
    date_of_last_watering: Option<NaiveDateTime>,
    date_of_aquirement: Option<NaiveDate>,
    notes: Option<String>,
}
#[derive(Deserialize)]
struct AddPlant {
    name: String,
    latin_name: String,
    family_id: i32,
    date_of_last_watering: Option<NaiveDateTime>,
    date_of_aquirement: Option<NaiveDate>,
    notes: Option<String>,
}

#[derive(Deserialize)]
struct AddFamily {
    name: String,
    latin_name: String,
    note: Option<String>,
}
#[derive(Serialize, FromRow)]
struct ReturnFamily{
    id: i32,
    name: String,
    latin_name: String,
    note: Option<String>
}


#[tokio::main]
async fn main(){
    // wczytywanie env
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tower_http=debug")),
        )
        .init();
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
        //Family
        .route("/api/family", post(post_family))
        .route("/api/family", get(get_familes))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server listining on :3000");
    tracing::info!("Server listeninr on :3000");

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
) -> Result<Json<Vec<PlantResponse>>, (StatusCode, String)> {
    let plants = sqlx::query_as::<_, PlantResponse>(
    "SELECT p.id, p.name, p.latin_name, p.family_id, f.name AS family_name,
        p.date_of_last_watering, p.date_of_aquirement, p.notes
        FROM plant p
        JOIN family f ON f.id = p.family_id",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Błąd podczas pobierania roślin z bazy");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Wewnętrzny błąd serwera".to_string(),
        )
    })?;

    tracing::debug!(count = plants.len(), "Pobrano rośliny");
    Ok(Json(plants))
}

async fn post_plants(
    State(state): State<AppState>,
    Json(payload): Json<AddPlant>,
) -> Result<(StatusCode, Json<PlantResponse>), (StatusCode, String)> {
    // Walidacja
    let name = payload.name.trim();
    let latin_name = payload.latin_name.trim();
    if name.is_empty() || latin_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Pola name i latin_name nie mogą być puste".to_string()
        ));
    }
    let plant = sqlx::query_as::<_, PlantResponse>(
        "WITH inserted AS (
             INSERT INTO plant (name, latin_name, family_id, date_of_last_watering, date_of_aquirement, notes)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING *
         )
         SELECT i.id, i.name, i.latin_name, i.family_id, f.name AS family_name,
                i.date_of_last_watering, i.date_of_aquirement, i.notes
         FROM inserted i
         JOIN family f ON f.id = i.family_id",
    )
    .bind(name)
    .bind(latin_name)
    .bind(&payload.family_id)
    .bind(&payload.date_of_last_watering)
    .bind(&payload.date_of_aquirement)
    .bind(&payload.notes)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e {
            match db_err.code().as_deref() {
                Some("23503") => {
                    tracing::warn!(family_id = payload.family_id, "Nieistniejąca rodzina");
                    return (StatusCode::UNPROCESSABLE_ENTITY, "Podana rodzina nie istnieje".to_string());
                }
                Some("23505") => {
                    tracing::warn!(name = %name, "Duplikat rośliny");
                    return (StatusCode::CONFLICT, "Taka roślina już istnieje".to_string());
                }
                Some("23502") | Some("23514") => {
                    tracing::warn!(error = %e, "Niepoprawne dane rośliny");
                    return (StatusCode::BAD_REQUEST, "Niepoprawne dane".to_string());
                }
                _ => {}
            }
        }
        tracing::error!(error = %e, "Błąd podczas dodawania rośliny");
        (StatusCode::INTERNAL_SERVER_ERROR, "Wewnętrzny błąd serwera".to_string())
    })?;

    Ok((StatusCode::CREATED, Json(plant)))
}

async fn post_family(
    State(state): State<AppState>,
    Json(payload): Json<AddFamily>,
) -> Result<(StatusCode, Json<ReturnFamily>), (StatusCode, String)> {

    // Walidacja 
    let name = payload.name.trim();
    let latin_name = payload.latin_name.trim();
    if name.is_empty() || latin_name.is_empty() {
        return  Err((
            StatusCode::BAD_REQUEST,
            "Pola name i latin_name nie mogą być puste".to_string(),
        ));

    }
    let family = sqlx::query_as::<_, ReturnFamily> (
        "INSERT INTO family (name, latin_name, note)
        VALUES ($1, $2, $3)
        RETURNING id, name, latin_name, note",
    )
    .bind(name)
    .bind(latin_name)
    .bind(&payload.note)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        eprintln!("Błąd podczs dodawania rodziny: {}", e);
        
        if let sqlx::Error::Database(db_err) = &e{
            match db_err.code().as_deref() {
                Some("23505") => {
                    return (StatusCode::CONFLICT, "Taka rodzina już istnieje".to_string())
                }
                Some("23502") | Some("23514") => {
                    return (StatusCode::BAD_REQUEST, "Niepoprawne dane".to_string())
                }
                _=> {}
            }
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Wewnętrzny błąd servera".to_string(),
        )
    })?;

    Ok((StatusCode::CREATED, Json(family)))
}

async fn get_familes(
    State(state): State<AppState>,
)-> Result<Json<Vec<ReturnFamily>>, (StatusCode, String)> {
    let famillies = sqlx::query_as::<_, ReturnFamily>(
        "SELECT id, name, latin_name, note FROM family"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Błąd podczas pobierania rodzin z bazy");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Wewnętrzny błąd serwera".to_string(),
        )
    })?;

    tracing::debug!(count = famillies.len(), "Pobrano rodziny roślin");
    Ok(Json(famillies))
}

