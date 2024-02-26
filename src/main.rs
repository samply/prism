mod beam;
mod config;
mod criteria;
mod errors;
mod logger;
mod mr;

use crate::errors::PrismError;
use crate::{config::CONFIG, mr::MeasureReport};
use std::collections::HashSet;
use std::io;
use std::process::exit;
use std::sync::Arc;
use std::time::SystemTime;
use http::HeaderValue;
use tokio::sync::Mutex;
use futures_util::{StreamExt as _, TryStreamExt};
use base64::engine::general_purpose::STANDARD as BASE64;

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};

use base64::Engine as _;
use once_cell::sync::Lazy;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tracing_subscriber::util::SubscriberInitExt;

use beam::create_beam_task;
use beam_lib::{AppId, BeamClient, MsgId};
use criteria::{combine_groups_of_criteria_groups, CriteriaGroups};
use std::{collections::HashMap, time::Duration};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn, Level};
use tracing_subscriber::EnvFilter;

use beam_lib::{RawString, TaskResult};

static BEAM_CLIENT: Lazy<BeamClient> = Lazy::new(|| {
    BeamClient::new(
        &CONFIG.beam_app_id_long,
        &CONFIG.api_key,
        CONFIG.beam_proxy_url.clone(),
    )
});

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LensQuery { // kept it the same as in spot, but only sites needed
    id: MsgId, // prism ignores this and creates its own
    sites: Vec<String>, //TODO: coordinate with Lens team to introduce a new type of lens query with sites only
    query: String, // prism ignores this and uses the default query for the project
}

type Site = String;
type Created = std::time::SystemTime; //epoch

#[derive(Debug, Clone)]
struct CriteriaCache {
    cache: HashMap<Site, (CriteriaGroups, Created)>,
}
const CRITERIACACHE_TTL: Duration = Duration::from_secs(86400); //cached criteria expire after 24h

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
    Prism doesn't return all the search criteria in the search tree and is not a replacement for MDR. It doesn't return criteria for which there are no results. It can't return results for range types. 
    
    It is not crucial that the counts are current or that they include all the BHs, speed of drawing lens is more important.
    At start prism sends a task to BHs in command line parameter and populates the cache.
    When lens sends a query, prism adds up results for BHs in the request which are present in cache (and not expired) and sends them to lens.
    Prism accumulates names of sites for which it doesn't have non-expired results in the cache in a set. 
    In a parallel process a task for all sites in the set is periodically sent to beam and a new process asking for the results is spawned.
    Successfully retrieved results are cached. 
       */


    let criteria_cache: CriteriaCache = CriteriaCache { //stores criteria for CRITERIACACHE_TTL to avoid querying the sites and processing results too often
        cache: HashMap::new(),
    };

    let sites_to_query: HashSet<String> = HashSet::new(); //accumulates sites to query, those for which Lens asked for criteria, and they either weren't cached or the cache had expired, emptied when task to sites sent

    let shared_state = SharedState {
        criteria_cache: Arc::new(Mutex::new(criteria_cache)),
        sites_to_query: Arc::new(Mutex::new(sites_to_query)),
    };

    if let Err(e) = logger::init_logger() {
        error!("Cannot initialize logger: {}", e);
        exit(1);
    };
    tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_env_filter(EnvFilter::from_default_env())
        .finish()
        .init();
    info!("{:#?}", &CONFIG);
    // TODO: check if beam up, if not exit

    if let Err(e) = wait_for_beam_proxy().await {
        error!("Beam doesn't work, it doesn't make sense that I run: {}", e);
        exit(2);
    }
    spawn_site_querying(shared_state.clone());

    let cors = CorsLayer::new()
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_origin(CONFIG.cors_origin.clone())
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/criteria", post(handle_get_criteria)) //here Lens asks for criteria for sites in its configuration
        .with_state(shared_state)
        .layer(cors);

    axum::Server::bind(&CONFIG.bind_addr)
        .serve(app.into_make_service())
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
    let mut criteria_groups: CriteriaGroups = CriteriaGroups::new(); // this is going to be aggregated criteria for all the sites

    for site in query.clone().sites {
        let criteria_groups_from_cache =
            match shared_state.criteria_cache.lock().await.cache.get(&site) {
                Some(cached) => {
                    //Prism only uses the cached results if they are not expired
                    if SystemTime::now().duration_since(cached.1).unwrap() < CRITERIACACHE_TTL {
                        Some(cached.0.clone())
                    } else {
                        None
                    }
                }
                None => None,
            };

            if let Some(cached_criteria_groups) = criteria_groups_from_cache { //cached and not expired
                criteria_groups = combine_groups_of_criteria_groups(criteria_groups, cached_criteria_groups); // adding all the criteria to the ones already in criteria_groups
            } else {    //not cached or expired
                shared_state.sites_to_query.lock().await.insert(site); // inserting the site into the set of sites to query
            }
    }

    let criteria_groups_json =
        serde_json::to_string(&criteria_groups).expect("Failed to serialize JSON");

    let response_builder = Response::builder().status(StatusCode::OK);

    Ok(response_builder
        .body(axum::body::Body::from(criteria_groups_json))
        .unwrap()
        .into_response())
}

async fn post_query(
    shared_state: SharedState,
    sites: Vec<String>,
) -> Result<(), PrismError> {
    if sites.is_empty() {
        info!("No sites to query");
        return Ok(());
    }
    let task = create_beam_task(sites);
    BEAM_CLIENT
        .post_task(&task)
        .await
        .map_err(|e| PrismError::BeamError(format!("Unable to post a query: {}", e)))?;

    tokio::spawn(async move { 
        if let Err(e) = get_results(shared_state, task.id).await {
            warn!("Failed to get results for {}: {e}", task.id);
        }
    });

    Ok(())
}

async fn query_sites(
    shared_state: SharedState,
    sites: Option<&[String]>,
) -> Result<(), PrismError> {

match sites{
        Some(sites) => { // argument site is present, Prism uses it and ignores sites from the shared state
            post_query(shared_state, sites.to_vec()).await?;
        },
        None => { // Prism queries sites from the shared state
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

async fn get_results(shared_state: SharedState, task_id: MsgId) -> Result<(), PrismError> {
    let criteria_cache: &mut tokio::sync::MutexGuard<'_, CriteriaCache> = &mut shared_state.criteria_cache.lock().await;
    let resp = BEAM_CLIENT
        .raw_beam_request(
            Method::GET,
            &format!("v1/tasks/{}/results?wait_count={}", task_id, CONFIG.wait_count),
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
    let mut stream = async_sse::decode(resp.bytes_stream().map_err(|e| io::Error::new(io::ErrorKind::Other, e)).into_async_read());
    while let Some(Ok(async_sse::Event::Message(msg))) = stream.next().await {
        let (from, measure_report) = match decode_result(&msg) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to deserialize message {msg:?} into a result: {e}");
                continue;
            },
        };
        let criteria = match mr::extract_criteria(measure_report) {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to extract criteria from {from}: {e}");
                continue;
            },
        };
        criteria_cache.cache.insert( //if successful caching the criteria
            from.app_name().into(), // extracting site name from app long name
            (criteria, std::time::SystemTime::now()),
        );
    }
    Ok(())
}

fn decode_result(msg: &async_sse::Message) -> anyhow::Result<(AppId, MeasureReport)> {
    let result: TaskResult<RawString> = serde_json::from_slice(msg.data())?;
    let decoded = BASE64.decode(result.body.0)?;
    Ok((result.from, serde_json::from_slice(&decoded)?))
}


async fn wait_for_beam_proxy() -> beam_lib::Result<()> {
    const MAX_RETRIES: u8 = 32;
    let mut tries = 1;
    loop {
        match reqwest::get(format!("{}/v1/health", CONFIG.beam_proxy_url)).await {
            Ok(res) if res.status() == StatusCode::OK => return Ok(()),
            _ if tries <= MAX_RETRIES => tries += 1,
            Err(e) => return Err(e.into()),
            Ok(res) => {
                return Err(beam_lib::BeamError::Other(
                    format!("Proxy reachable but failed to start {}", res.status()).into(),
                ))
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
