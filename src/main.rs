mod beam;
mod config;
mod criteria;
mod errors;
mod logger;
mod mr;

use crate::errors::PrismError;
use crate::{config::CONFIG, mr::MeasureReport};
use std::collections::HashSet;
use std::process::exit;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::Mutex;

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};

use base64::engine::general_purpose;
use base64::Engine as _;
use once_cell::sync::Lazy;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use tracing_subscriber::util::SubscriberInitExt;

use beam::create_beam_task;
use beam_lib::{BeamClient, MsgId};
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

type Attempts = usize;

const MAX_ATTEMPTS: usize = 3; // maximum number of attempts to get results for each task

#[derive(Clone)]
struct SharedState {
    criteria_cache: Arc<Mutex<CriteriaCache>>,
    sites_to_query: Arc<Mutex<HashSet<String>>>,
    tasks: Arc<Mutex<HashMap<MsgId, usize>>>,
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
    Prism accumulates names of sites for which it doesn't have non-expired results in the cache in a set. In a parallel process a task for all sites in the set is periodically sent to beam. 
    After a task is sent successfully its task id is added to the list of tasks for which Prism needs to get the results. That list also stores how many times Prism has attempted to get the results for each task.
    In another parallel process beam is asked for results for all the tasks in the list of tasks. 
    Successfully retrieved results are cached. In case of an error, the number of attempts in the list for the task id is increased, provided that it's not higher than the maximum number of attempts, otherwise task id is removed from the list.
    */

    // TODO: start a thread/process/worker for posting tasks to beam
    // TODO: start a thread/process/worker for getting results from beam and processing them
    // TODO: handle errors in main

    let criteria_cache: CriteriaCache = CriteriaCache { //stores criteria for CRITERIACACHE_TTL to avoid querying the sites and processing results too often
        cache: HashMap::new(),
    };

    let sites_to_query: HashSet<String> = HashSet::new(); //accumulates sites to query, those for which Lens asked for criteria, and they either weren't cached or the cache had expired, emptied when task to sites sent

    let tasks: HashMap<MsgId, Attempts> = HashMap::new(); //accumulates IDs of tasks sent and the numbers of attempts of getting the results of them, emptied when results gotten or the maximum number of attempts exceeded 

    let shared_state = SharedState {
        criteria_cache: Arc::new(Mutex::new(criteria_cache)),
        sites_to_query: Arc::new(Mutex::new(sites_to_query)),
        tasks: Arc::new(Mutex::new(tasks)),
    };

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
    // TODO: check if beam up, if not exit

    let result = query_sites(&shared_state, Some(&CONFIG.sites)).await; // querying sites from configuration to populate the cache at start

    match result {
        Ok(()) => {}
        Err(e) => {
            error!("Beam doesn't work, it doesn't make sense that I run: {}", e);
            exit(2);
        }
    }

    let cors = CorsLayer::new()
        .allow_methods([http::Method::GET, http::Method::POST])
        .allow_origin(CONFIG.cors_origin.clone())
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/criteria", post(handle_get_criteria)) //here Lens asks for criteria for sites in its configuration
        .with_state(shared_state)
        .layer(cors);

    let tcp_listener = tokio::net::TcpListener::bind(CONFIG.bind_addr)
        .await
        .expect("Unable to bind to listen port");

    axum::serve(tcp_listener, app.into_make_service())
        .await
        .unwrap();
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
    mut tasks: tokio::sync::MutexGuard<'_, HashMap<MsgId, usize>>,
    sites: &[impl ToString],
) -> Result<(), PrismError> {
    let task = create_beam_task(sites);
    BEAM_CLIENT
        .post_task(&task)
        .await
        .map_err(|e| PrismError::BeamError(format!("Unable to post a query: {}", e)))?;

    tasks.insert(task.id, 0); // if beam task is successfully created, its id is added to the list of tasks to get the results of

    Ok(())
}

async fn query_sites(
    shared_state: &SharedState,
    sites: Option<&[impl ToString]>,
) -> Result<(), PrismError> {

match sites{
        Some(sites) => { // argument site is present, Prism uses it and ignores sites from the shared state
            post_query(shared_state.tasks.lock().await, sites).await?;
        },
        None => { // Prism queries sites from the shared state
            let mut locked_sites = shared_state.sites_to_query.lock().await;
            let sites: Vec<String> = locked_sites.clone().into_iter().collect();
            if sites.is_empty() {
                return (Ok(()));
            }
            post_query(shared_state.tasks.lock().await, &sites).await?;
            locked_sites.clear(); // if posting the task was successful, the set of sites to query is emptied
        }
    };

    Ok(())
}

async fn get_results(shared_state: SharedState) -> Result<(), PrismError> {

    let mut locked_tasks = shared_state.tasks.lock().await;
    for task in locked_tasks.clone() {
        let processed = process_results(task.0, &mut shared_state.criteria_cache.lock().await).await;
        match processed {
            Ok(()) => {
                locked_tasks.remove(&task.0); // results received and cached, task id removed from the list
            }
            Err(e) => {
                error!("There has been an error getting results for task {}. Error: {}", task.0, e);

                if task.1 > MAX_ATTEMPTS - 1 {
                    locked_tasks.remove(&task.0); // results not received, but max number of attempts reached, task id removed from the list
                } else {
                    locked_tasks.entry(task.0).and_modify(|e| *e += 1); // results not received, max number of attempts not reached, the number of attempts in the list is increased
                }
            }
        }
    }

    Ok(())
}

async fn process_results(
    task: MsgId,
    criteria_cache: &mut tokio::sync::MutexGuard<'_, CriteriaCache>,
) -> Result<(), PrismError> {
    let resp = BEAM_CLIENT
        .raw_beam_request(
            Method::GET,
            &format!("v1/tasks/{}/results?wait_count={}", task, CONFIG.wait_count),
        )
        .header(
            http_old::header::ACCEPT,
            http_old::HeaderValue::from_static("text/event-stream"),
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

    let text = resp
        .text()
        .await
        .map_err(|e| PrismError::BeamError(e.to_string()))?;

    let task_result_result: Result<TaskResult<RawString>, PrismError> =
        serde_json::from_str(&text).map_err(|e| PrismError::DeserializationError(e.to_string()));

    let task_result = task_result_result?;

    let decoded: Result<Vec<u8>, PrismError> = general_purpose::STANDARD
        .decode(task_result.body.into_string())
        .map_err(PrismError::DecodeError);

    let vector = decoded?;

    let measure_report_result: Result<MeasureReport, PrismError> = serde_json::from_slice(&vector)
        .map_err(|e| PrismError::DeserializationError(e.to_string()));

    let measure_report = measure_report_result?;

    let criteria = mr::extract_criteria(measure_report)?; // extracting criteria from measure report

    criteria_cache.cache.insert( //if successful caching the criteria
        task_result.from.app_name().into(), // extracting site name from app long name
        (criteria, std::time::SystemTime::now()),
    );

    Ok(())
}
