

use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response,};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use base64::engine::general_purpose;
use base64::Engine;
use rand::Rng;
use sqlx::PgPool;
use url::Url;

use crate::utils::internal_error;

const DEFAULT_CACHE_CONTROL_HEADER_VALUE: &str = 
    "public, max-age=300, s-maxage=300, stale-while-revalidate=300, stale-if-error=300";

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
     pub id: String,
     pub target_url: String
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTarget {
    pub target_url: String
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountedLinkStatistic {
    pub amount: Option<i64>,
    pub referer: Option<String>,
    pub user_agent: Option<String>
}

fn generate_id() -> String {
    let random_number = rand::thread_rng().gen_range(0..u32::MAX);
    general_purpose::URL_SAFE_NO_PAD.encode(random_number.to_string())
}

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "Service is healthy")
}

pub async fn redirect(
    State(pool): State<PgPool>,
    Path(requested_link): Path<String>,
    headers: HeaderMap
) -> Result<Response, (StatusCode, String)> {

    let select_timeout = tokio::time::Duration::from_millis(300);

    let link = tokio::time::timeout(
        select_timeout, 
        sqlx::query_as!(
        Link, 
        "select id, target_url from links where id = $1",
        requested_link
    )
    .fetch_optional(&pool)
)
        .await
        .map_err(internal_error)?
        .map_err(internal_error)?
        .ok_or_else(|| "Not found".to_string())
        .map_err(|err| (StatusCode::NOT_FOUND, err))?;

    tracing::debug!(
        "Redirecting link id {} to {}",
        requested_link,
        link.target_url
    );

    let referer_header = headers
        .get("referer")
        .map(|value| value.to_str().unwrap_or_default().to_string());

    let user_agent_header = headers
        .get("user-agent")
        .map(|value| value.to_str().unwrap_or_default().to_string());

    let insert_statistics_timeout = tokio::time::Duration::from_millis(300);

    let saved_statistic = tokio::time::timeout(
        insert_statistics_timeout,
        sqlx::query(
            r#"
                insert into link_statistics(link_id, referer, user_agent)
                values($1, $2, $3)
            "#
        )
        .bind(&requested_link)
        .bind(&referer_header)
        .bind(&user_agent_header)
        .execute(&pool)
    )
    .await;

    match saved_statistic {
        Err(elapsed) => tracing::error!("Saving new link click resulted in timeout: {}", elapsed),
        Ok(Err(err)) => tracing::error!(
            "Saving a new link click failed with the following error: {}",
            err
        ),
        _ => tracing::debug!(
            "Persisted new link click for link with id {}, referer {} and user-agent {}",
            requested_link,
            referer_header.unwrap_or_default(),
            user_agent_header.unwrap_or_default()
        )
    };

    Ok(
        Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header("location", link.target_url)
        .header("Cache-Control", DEFAULT_CACHE_CONTROL_HEADER_VALUE)
        .body(Body::empty())
        .expect("This response should always be constructable")
    )
}


pub async fn create_link(
    State(pool): State<PgPool>,
    Json(new_link): Json<LinkTarget>
) -> Result<Json<Link>, (StatusCode, String)> {
    let url = Url::parse(&new_link.target_url)
    .map_err(|_| (StatusCode::CONFLICT, "url malformed".into()))?
    .to_string();

    let new_link_id = generate_id();

    let insert_link_timeout = tokio::time::Duration::from_millis(300);

    let new_link = tokio::time::timeout(
        insert_link_timeout, 
        sqlx::query_as!(
            Link,
            r#"
            with inserted_link as (
                insert into links(id, target_url)
                values($1, $2)
                returning id, target_url
            ) select id, target_url from inserted_link
            "#,
            &new_link_id,
            &url
        )
        .fetch_one(&pool)
    )
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    tracing::debug!("Created new link with id {} targeting {}", new_link_id, url);

    Ok(Json(new_link))
    
}

pub async fn update_link(
    State(pool): State<PgPool>,
    Path(link_id): Path<String>,
    Json(update_link): Json<LinkTarget>
) -> Result<Json<Link>, (StatusCode, String)> {
    let url = Url::parse(&update_link.target_url)
        .map_err(|_| (StatusCode::CONFLICT, "url malformed".into()))?
        .to_string();

    let update_link_timeout = tokio::time::Duration::from_millis(300);

    let updated_link = tokio::time::timeout(
        update_link_timeout, 
        sqlx::query_as!(
            Link,
            r#"
                with updated_link as (
                    update links set target_url = $1 where id = $2
                    returning id, target_url
                ) select id, target_url from updated_link
            "#,
            &url,
            &link_id
        )
        .fetch_one(&pool)
    )
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    tracing::debug!("Updated link with id {} targeting {}", link_id, url);

    Ok(Json(updated_link))
}

pub async fn get_link_statistic(
    State(pool): State<PgPool>,
    Path(link_id): Path<String>,
) -> Result<Json<Vec<CountedLinkStatistic>>, (StatusCode, String)> {
    let fetch_statistice_timeout = tokio::time::Duration::from_millis(300);

    let statistics = tokio::time::timeout(
        fetch_statistice_timeout,
        sqlx::query_as!(
            CountedLinkStatistic,
            r#"
                select count(*) as amount, referer, user_agent from link_statistics group by link_id, referer, user_agent having link_id = $1
            "#,
            &link_id
        )
        .fetch_all(&pool)
    )
    .await
    .map_err(internal_error)?
    .map_err(internal_error)?;

    tracing::debug!("Statistics for link with id {} requested", link_id);

    Ok(Json(statistics))
}