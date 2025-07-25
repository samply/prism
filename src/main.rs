mod beam;
mod config;
mod criteria;
mod errors;
mod logger;
mod measure_report;

use crate::errors::PrismError;
use crate::{config::CONFIG, measure_report::extract_criteria, measure_report::MeasureReport};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use futures_util::{StreamExt as _, TryStreamExt};
use std::collections::HashSet;
use std::io;
use std::process::exit;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::{net::TcpListener, sync::Mutex};

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use reqwest::{header, header::HeaderValue, Method};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use beam::create_beam_task;
use beam_lib::{AppId, BeamClient, MsgId};
use criteria::{combine_criteria_groups, Stratifiers};
use std::{collections::HashMap, time::Duration};
use tower_http::cors::CorsLayer;
use tracing::{debug, error, info, warn};

use beam_lib::{RawString, TaskResult};

static BEAM_CLIENT: Lazy<BeamClient> = Lazy::new(|| {
    BeamClient::new(
        &CONFIG.beam_app_id_long,
        &CONFIG.api_key,
        CONFIG.beam_proxy_url.clone(),
    )
});

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LensQuery {
    sites: Vec<String>,
}

type Site = String;
type Created = std::time::SystemTime; //epoch

#[derive(Debug, Clone)]
struct CriteriaCache {
    cache: HashMap<Site, (Stratifiers, Created)>,
}
const CRITERIACACHE_TTL: Duration = Duration::from_secs(7200); //cached criteria expire after 2h

#[derive(Clone)]
struct SharedState {
    criteria_cache: Arc<Mutex<CriteriaCache>>,
    sites_to_query: Arc<Mutex<HashSet<String>>>,
}

#[tokio::main]
pub async fn main() {
    /*
    🏳️‍🌈⃤
    Prism returns cumulative positive numbers of each of individual criteria defined in CQL queries and Measures at sites it is queried about.
    Prism doesn't return all the search criteria in the search tree and is not a replacement for MDR.
    It doesn't return criteria for which there are no results. It can't return results for range types.

    It is not crucial that the counts are current or that they include all the BHs, speed of drawing lens is more important.
    At start prism sends a task to BHs in command line parameter and populates the cache.
    When lens sends a query, prism adds up results for BHs in the request which are present in cache (and not expired) and sends them to lens.
    Prism accumulates names of sites for which it doesn't have non-expired results in the cache in a set.
    In a parallel process a task for all sites in the set is periodically sent to beam and a new process asking for the results is spawned.
    Successfully retrieved results are cached.
    */

    let criteria_cache: CriteriaCache = CriteriaCache {
        //stores criteria for CRITERIACACHE_TTL to avoid querying the sites and processing results too often
        cache: HashMap::new(),
    };

    let sites_to_query: HashSet<String> = HashSet::new();
    //accumulates sites to query, those for which Lens asked for criteria, and they either weren't cached or the cache had expired, emptied when task to sites sent

    let shared_state = SharedState {
        criteria_cache: Arc::new(Mutex::new(criteria_cache)),
        sites_to_query: Arc::new(Mutex::new(sites_to_query)),
    };

    if let Err(e) = logger::init_logger() {
        error!("Cannot initialize logger: {}", e);
        exit(1);
    };

    if let Err(e) = wait_for_beam_proxy().await {
        error!("Beam doesn't work, it doesn't make sense that I run: {}", e);
        exit(2);
    }

    info!("Beam ready");

    spawn_site_querying(shared_state.clone());

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(CONFIG.cors_origin.clone())
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/criteria", post(handle_get_criteria)) //here Lens asks for criteria for sites in its configuration
        .with_state(shared_state)
        .layer(cors);

    axum::serve(
        TcpListener::bind(CONFIG.bind_addr).await.unwrap(),
        app.into_make_service(),
    )
    .await
    .unwrap()
}

fn spawn_site_querying(shared_state: SharedState) {
    tokio::spawn(async move {
        if let Err(e) = query_sites(shared_state.clone(), Some(&CONFIG.sites)).await {
            warn!("Failed to query sites: {e}. Will try again later");
        }
        loop {
            if let Err(e) = query_sites(shared_state.clone(), None).await {
                warn!("Failed to query sites: {e}. Will try again later");
            }
            tokio::time::sleep(Duration::from_secs(15 * 60)).await;
        }
    });
}

async fn handle_get_criteria(
    State(shared_state): State<SharedState>,
    Json(query): Json<LensQuery>,
) -> Result<Response, (StatusCode, String)> {
    let mut stratifiers: Stratifiers = Stratifiers::new(); // this is going to be aggregated criteria for all the sites

    let mut sites = query.sites;

    // allowing empty list of sites in the request because Spot is going to query with the empty list and expect response for the sites in Prism's config

    if sites.is_empty() {
        sites = CONFIG.sites.clone();
    }

    for site in sites {
        debug!("Request for site {}", &site);
        let stratifiers_from_cache = match shared_state.criteria_cache.lock().await.cache.get(&site)
        {
            Some(cached) => {
                //Prism only uses the cached results if they are not expired
                debug!("Results for site {} found in cache", &site);
                if SystemTime::now().duration_since(cached.1).unwrap() < CRITERIACACHE_TTL {
                    Some(cached.0.clone())
                } else {
                    debug!(
                        "Results for site {} in cache sadly expired, will query again",
                        &site
                    );
                    None
                }
            }
            None => {
                debug!("Results for site {} in cache not found in cache", &site);
                None
            }
        };

        if let Some(cached_stratifiers) = stratifiers_from_cache {
            //cached and not expired
            stratifiers = combine_criteria_groups(stratifiers, cached_stratifiers);
        // adding all the criteria to the ones already in criteria_groups
        } else {
            //not cached or expired
            shared_state.sites_to_query.lock().await.insert(site); // inserting the site into the set of sites to query
        }
    }

    let stratifiers_json = serde_json::to_string(&stratifiers).expect("Failed to serialize JSON");

    let response_builder = Response::builder().status(StatusCode::OK);

    Ok(response_builder
        .body(axum::body::Body::from(stratifiers_json))
        .unwrap()
        .into_response())
}

async fn post_query(shared_state: SharedState, sites: Vec<String>) -> Result<(), PrismError> {
    if sites.is_empty() {
        info!("No sites to query");
        return Ok(());
    }
    let wait_count = sites.len();
    let site_display = sites.join(", ");
    let task = create_beam_task(sites);
    info!("Querying sites {:?}", site_display);
    BEAM_CLIENT
        .post_task(&task)
        .await
        .map_err(|e| PrismError::BeamError(format!("Unable to post a query: {}", e)))?;

    info!("Posted task {}", task.id);

    tokio::spawn(async move {
        if let Err(e) = get_results(shared_state, task.id, wait_count).await {
            warn!("Failed to get results for {}: {e}", task.id);
        }
    });

    Ok(())
}

async fn query_sites(
    shared_state: SharedState,
    sites: Option<&[String]>,
) -> Result<(), PrismError> {
    match sites {
        Some(sites) => {
            // argument site is present, Prism uses it and ignores sites from the shared state
            post_query(shared_state, sites.to_vec()).await?;
        }
        None => {
            // Prism queries sites from the shared state
            let mut locked_sites = shared_state.sites_to_query.lock().await;
            let sites: Vec<String> = locked_sites.clone().into_iter().collect();
            if sites.is_empty() {
                return Ok(());
            }
            post_query(shared_state.clone(), sites).await?;
            locked_sites.clear(); // if posting the task was successful, the set of sites to query is emptied
        }
    };

    Ok(())
}

async fn get_results(
    shared_state: SharedState,
    task_id: MsgId,
    wait_count: usize,
) -> Result<(), PrismError> {
    let resp = BEAM_CLIENT
        .raw_beam_request(
            Method::GET,
            &format!("v1/tasks/{}/results?wait_count={}", task_id, wait_count),
        )
        .header(
            header::ACCEPT,
            HeaderValue::from_static("text/event-stream"),
        )
        .send()
        .await
        .map_err(|e| PrismError::BeamError(e.to_string()))?;

    let code = resp.status();
    if !code.is_success() {
        return Err(PrismError::BeamError(
            resp.text().await.unwrap_or_else(|e| e.to_string()),
        ));
    }
    let mut stream = async_sse::decode(
        resp.bytes_stream()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
            .into_async_read(),
    );
    while let Some(Ok(async_sse::Event::Message(msg))) = stream.next().await {
        let (from, measure_report) = match decode_result(&msg) {
            Ok(v) => v,
            Err(PrismError::UnexpectedWorkStatus(beam_lib::WorkStatus::Claimed)) => {
                info!("Task claimed");
                continue;
            }
            Err(PrismError::UnexpectedWorkStatus(
                beam_lib::WorkStatus::PermFailed | beam_lib::WorkStatus::TempFailed,
            )) => {
                warn!("WorkStatus PermFailed: {msg:?}");
                continue;
            }
            Err(e) => {
                warn!("Failed to deserialize message {msg:?} into a result: {e}");
                continue;
            }
        };
        let criteria = match extract_criteria(measure_report) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to extract criteria from {from}: {e}");
                continue;
            }
        };
        shared_state.criteria_cache.lock().await.cache.insert(
            //if successful caching the criteria
            from.as_ref().split('.').nth(1).unwrap().to_string(), // extracting site name from app long name
            (criteria, std::time::SystemTime::now()),
        );
        info!(
            "Cached results from site {} for task {}",
            from.as_ref().split('.').nth(1).unwrap().to_string(),
            task_id
        );
    }
    Ok(())
}

fn decode_result(msg: &async_sse::Message) -> Result<(AppId, MeasureReport), PrismError> {
    let result: TaskResult<RawString> =
        serde_json::from_slice(msg.data()).map_err(PrismError::DeserializationError)?;
    match result.status {
        beam_lib::WorkStatus::Succeeded => {}
        yep => {
            // claimed not an error!!!!
            return Err(PrismError::UnexpectedWorkStatus(yep));
        }
    }
    let decoded = BASE64
        .decode(result.body.0)
        .map_err(PrismError::DecodeError)?;
    Ok((
        result.from,
        serde_json::from_slice(&decoded).map_err(PrismError::DeserializationError)?,
    ))
}

async fn wait_for_beam_proxy() -> beam_lib::Result<()> {
    const MAX_RETRIES: u8 = 10;
    let mut tries = 1;
    loop {
        match reqwest::get(format!("{}v1/health", CONFIG.beam_proxy_url)).await {
            //FIXME why doesn't it work with url from config
            Ok(res) if res.status() == reqwest::StatusCode::OK => return Ok(()),
            _ if tries <= MAX_RETRIES => tries += 1,
            Err(e) => return Err(e.into()),
            Ok(res) => {
                return Err(beam_lib::BeamError::Other(
                    format!("Proxy reachable but failed to start {}", res.status()).into(),
                ));
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
