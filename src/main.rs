mod beam;
mod config;
mod errors;
mod logger;
mod criteria;
mod mr;

use crate::{config::CONFIG, mr::MeasureReport};
use crate::errors::PrismError;
use std::sync::{Arc, Mutex};
use std::process::{exit, ExitCode};

use axum::{
    extract::{Json, Path, Query},
    http::HeaderValue,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use beam::create_beam_task;
use beam_lib::{BeamClient, MsgId};
use criteria::CriteriaGroup;
use once_cell::sync::Lazy;
use reqwest::{header, Method, StatusCode};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn, Level};
use tracing_subscriber::{util::SubscriberInitExt, EnvFilter};
use std::{collections::HashMap, time::Duration};

use beam_lib::{TaskRequest, TaskResult};

static BEAM_CLIENT: Lazy<BeamClient> = Lazy::new(|| {
    BeamClient::new(
        &CONFIG.beam_app_id_long,
        &CONFIG.api_key,
        CONFIG.beam_proxy_url.clone(),
    )
});

#[derive(Serialize, Deserialize)]
struct LensQuery {
    id: MsgId,
    sites: Vec<String>,
    query: String,
}

type Site = String;
type Created = std::time::SystemTime; //epoch

#[derive(Debug, Clone)]
struct CriteriaCache {
    cache: HashMap<Site, (Vec<CriteriaGroup>, Created)>,
}
const REPORTCACHE_TTL: Duration = Duration::from_secs(86400); //24h

#[tokio::main]
async fn main() {

    if let Err(e) = logger::init_logger() {
        error!("Cannot initalize logger: {}", e);
        exit(1);
    };
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_env_filter(EnvFilter::from_default_env())
        .finish()
        .init();
    info!("{:#?}", &CONFIG);
    // TODO: Add check for reachability of beam-proxy

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(CONFIG.cors_origin.clone())
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/beam", post(handle_create_beam_task))
        .route("/beam/:task_id", get(handle_listen_to_beam_tasks))
        //.layer(axum::middleware::map_response(set_server_header))
        .layer(cors);

    axum::Server::bind(&CONFIG.bind_addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handle_create_beam_task(
    Json(query): Json<LensQuery>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    let LensQuery { id: _, sites, query: _ } = query;
    let task = create_beam_task(sites);
    BEAM_CLIENT.post_task(&task).await.map_err(|e| {
        warn!("Unable to query Beam.Proxy: {}", e);
        (StatusCode::BAD_GATEWAY, "Unable to query Beam.Proxy")
    })?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
struct ListenQueryParameters {
    wait_count: u16,
}

async fn handle_listen_to_beam_tasks(
    Path(task_id): Path<MsgId>,
    Query(listen_query_parameter): Query<ListenQueryParameters>
)  {
    let resp = BEAM_CLIENT
        .raw_beam_request(
            Method::GET,
            &format!(
                "v1/tasks/{}/results?wait_count={}",
                task_id, listen_query_parameter.wait_count
            ),
        )
        .header(
            header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        )
        .send()
        .await
        .map_err(|err| {
            println!(
                "Failed request to {} with error: {}",
                CONFIG.beam_proxy_url, err
            );
            (
                StatusCode::BAD_GATEWAY,
                "Error calling beam, check the server logs.".to_string(),
            )
        });
    
    if let Err(e) = resp {
        warn!("Resp is messed up");
        return;
    }

    let resp = resp.unwrap();

    let code = resp.status();
    if !code.is_success() {
        //return Err((code, ));

        //just log the error, no return necessary
        warn!("Error: {}", resp.text().await.unwrap_or_else(|e| e.to_string()));
        return;
        
    }
    //here convert from MR to criteria and add to cache

    let body = resp.text().await;

    if let Err(e) = body {
        warn!("Error: {}", e.to_string());
        return;
    }

    let measure_report: Result<mr::MeasureReport, PrismError> = serde_json::from_str(body.unwrap().as_str()).map_err(|e| PrismError::DeserializationError(e.to_string()));

    let measure_report = measure_report.unwrap();

    let criteria = mr::extract_criteria(measure_report);

    //if let Err(criteria) = body {
        //warn!("Error extracting criteria from MeasureReport {}", e.to_string());
        //return;
    //}

    let criteria = criteria.unwrap();

    //criteria_cache; 





    //Ok(convert_response(resp))
}

// Modified version of https://github.com/tokio-rs/axum/blob/c8cf147657093bff3aad5cbf2dafa336235a37c6/examples/reqwest-response/src/main.rs#L61
fn convert_response(response: reqwest::Response) -> axum::response::Response {
    let mut response_builder = Response::builder().status(response.status());

    // This unwrap is fine because we haven't insert any headers yet so there can't be any invalid
    // headers
    *response_builder.headers_mut().unwrap() = response.headers().clone();

    response_builder
        .body(axum::body::Body::wrap_stream(response.bytes_stream()))
        // Same goes for this unwrap
        .unwrap()
        .into_response()
}

//it is not crucial that the counts are current or that they include all the BHs, speed of drawing lens is more important
//at start prism sends a task to BHs in command line parameter and populates the cache
//when lens sends a query, prism adds up results for BHs in the request which it has in cache and sends them to lens
//prism sends queries to BHs from the request it doesn't have in cache (or are expired) and updates the cache


//🏳️‍🌈⃤

pub(crate) async fn set_server_header<B>(mut response: Response<B>) -> Response<B> {
    if !response.headers_mut().contains_key(header::SERVER) {
        response.headers_mut().insert(
            header::SERVER,
            HeaderValue::from_static("Samply :)"),
        );
    }
    response
}

