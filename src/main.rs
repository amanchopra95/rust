mod routes;
mod utils;
mod auth;

use std::error::Error;

use axum::{middleware, routing::{get, patch, post}, Router};
use axum_prometheus::PrometheusMetricLayer;
use routes::{create_link, get_link_statistic, health, redirect, update_link};
use sqlx::postgres::PgPoolOptions;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use dotenvy::dotenv;
use auth::auth;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {

    dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "link_shortener=debug".into())
        )
        .with(tracing_subscriber::fmt::layer())
        .init();


    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL is required");

    let db_conn = PgPoolOptions::new()
        .max_connections(20)
        .connect(&db_url)
        .await?;

    let (prometheous_layer, metric_handle) = PrometheusMetricLayer::pair();

    let app: Router<()> = Router::new()
        .route("/create", post(create_link))
        .route("/:id/statistics", get(get_link_statistic))
        .route_layer(middleware::from_fn_with_state(db_conn.clone(), auth))
        .route("/:id", 
            patch(update_link)
            .route_layer(middleware::from_fn_with_state(db_conn.clone(), auth))
            .get(redirect))
        .route("/metrics", get(|| async move {metric_handle.render()}))
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .layer(prometheous_layer)
        .with_state(db_conn);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Could not initialize TcpListner");

    tracing::debug!(
        "listening on {}",
        listener
        .local_addr()
        .expect("Could not convert listener address to local address")
    );

    axum::serve(listener, app)
    .await
    .expect("Could initiate server");
    
    Ok(())
}
