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
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use axum::{
    extract::State, 
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, 
    Router,
};

use base64::engine::general_purpose;
use base64::Engine as _;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tracing_subscriber::util::SubscriberInitExt;

use beam::create_beam_task;
use beam_lib::{BeamClient, MsgId};
use criteria::{combine_groups_of_criteria_groups, CriteriaGroups};
use reqwest::Method;
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
struct LensQuery {
    id: MsgId,
    sites: Vec<String>,
    query: String,
}

type Site = String;
type Created = std::time::SystemTime; //epoch

#[derive(Debug, Clone)]
struct CriteriaCache {
    cache: HashMap<Site, (CriteriaGroups, Created)>,
}
const CRITERIACACHE_TTL: Duration = Duration::from_secs(86400); //24h

#[derive(Clone)]
struct SharedState {
    criteria_cache: Arc<Mutex<CriteriaCache>>,
    sites_to_query: Arc<Mutex<HashSet<String>>>,
    tasks: Arc<Mutex<HashMap<MsgId, usize>>>,
}

#[tokio::main]
pub async fn main() {
    //🏳️‍🌈⃤
    //it is not crucial that the counts are current or that they include all the BHs, speed of drawing lens is more important
    //at start prism sends a task to BHs in command line parameter and populates the cache
    //when lens sends a query, prism adds up results for BHs in the request which it has in cache and sends them to lens
    //prism sends queries to BHs from the request it doesn't have in cache (or are expired) and updates the cache

    let criteria_cache: CriteriaCache = CriteriaCache {
        cache: HashMap::new(),
    };

    let sites_to_query: HashSet<String> = HashSet::new();

    let tasks: HashMap<MsgId, usize> = HashMap::new();

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

    let result = query_sites(shared_state, Some(CONFIG.sites)).await;

    match result {
        Ok(()) => {}
        Err(e) => {
            error!("Beam doesn't work, it doesn't make sense that I run: {}", e);
            exit(2);
        }
    }

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(CONFIG.cors_origin.clone())
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/criteria", post(handle_get_criteria))
        .with_state(shared_state)
        .layer(cors);

    axum::Server::bind(&CONFIG.bind_addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[axum_macros::debug_handler]
async fn handle_get_criteria(
    State(shared_state): State<SharedState>,
    Json(query): Json<LensQuery>,
) -> Result<Response, (StatusCode, String)> {
    let mut criteria_groups: CriteriaGroups = CriteriaGroups::new();

    match shared_state.criteria_cache.lock() {
        Err(e) => {
            error!("Mutex was poisoned: {:?}", e);
        }
        Ok(criteria_cache) => {
            for site in query.clone().sites {
                if criteria_cache.cache.contains_key(&site) {
                    let maybe_criteria_group_time_from_cache = criteria_cache.cache.get(&site);
                    match maybe_criteria_group_time_from_cache {
                        //would just unwrap it, but unwrap can be a minefield after refactoring
                        None => match shared_state.sites_to_query.lock() {
                            Err(e) => {
                                error!("Mutex was poisoned: {:?}", e);
                            }
                            Ok(mut sites_to_query) => {
                                sites_to_query.insert(site);
                            }
                        },
                        Some(criteria_groups_time_from_cache) => {
                            if SystemTime::now()
                                .duration_since(criteria_groups_time_from_cache.1)
                                .unwrap()
                                < CRITERIACACHE_TTL
                            {
                                criteria_groups = combine_groups_of_criteria_groups(
                                    criteria_groups,
                                    criteria_groups_time_from_cache.clone().0,
                                );
                            } else {
                                match shared_state.sites_to_query.lock() {
                                    Err(e) => {
                                        error!("Mutex was poisoned: {:?}", e);
                                    }
                                    Ok(mut sites_to_query) => {
                                        sites_to_query.insert(site);
                                    }
                                }
                            }
                        }
                    }
                }
            }
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
    mut tasks: std::sync::MutexGuard<'_, HashMap<MsgId, usize>>,
    sites: Vec<String>,
) -> Result<(), PrismError> {
    let task = create_beam_task(sites);
    BEAM_CLIENT
        .post_task(&task)
        .await
        .map_err(|e| PrismError::BeamError(format!("Unable to post a query: {}", e)))?;

    tasks.insert(task.id, 0);

    Ok(())
}

async fn query_sites(
    shared_state: SharedState,
    sites: Option<Vec<String>>,
) -> Result<(), PrismError> {
    if let Some(sites) = sites {
        match shared_state.tasks.lock() {
            Err(e) => {
                error!("Mutex was poisoned: {:?}", e);
                return Err(PrismError::PoisonedMutex(e.to_string()));
            }
            Ok(tasks) => {
                post_query(tasks, sites.clone().into_iter().collect()).await?;
            }
        }
    }

    match shared_state.sites_to_query.lock() {
        Err(e) => {
            error!("Mutex was poisoned: {:?}", e);
            return Err(PrismError::PoisonedMutex(e.to_string()));
        }
        Ok(mut sites_to_query) => {
            match shared_state.tasks.lock() {
                Err(e) => {
                    error!("Mutex was poisoned: {:?}", e);
                    return Err(PrismError::PoisonedMutex(e.to_string()));
                }
                Ok(tasks) => {
                    post_query(tasks, sites_to_query.clone().into_iter().collect()).await?;
                    sites_to_query.clear(); //only if no error
                }
            }
        }
    }

    Ok(())
}

async fn get_results(shared_state: SharedState) -> Result<(), PrismError> {
    match shared_state.tasks.lock() {
        Err(e) => {
            error!("Mutex was poisoned: {:?}", e);
            return Err(PrismError::PoisonedMutex(e.to_string()));
        }
        Ok(mut tasks) => {
            match shared_state.criteria_cache.lock() {
                Err(e) => {
                    error!("Mutex was poisoned: {:?}", e);
                    return Err(PrismError::PoisonedMutex(e.to_string()));
                }
                Ok(mut criteria_cache) => {
                    for task in tasks.clone().into_iter() {
                        let processed = process_task(task.0, &mut criteria_cache).await;
                        match processed {
                            Ok(()) => {
                                tasks.remove(&task.0);
                            }
                            Err(e) => {
                                error!("There has been an error getting results for task {}. Error: {}", task.0, e);

                                if task.1 > 3 {
                                    tasks.remove(&task.0); // 3 attempts enough, could even be 1
                                } else {
                                    tasks.entry(task.0).and_modify(|e| *e += 1);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn process_task(task: MsgId, criteria_cache: &mut CriteriaCache) -> Result<(), PrismError> {
    let resp = BEAM_CLIENT
        .raw_beam_request(
            Method::GET,
            &format!("v1/tasks/{}/results?wait_count={}", task, CONFIG.wait_count),
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

    let text = resp
        .text()
        .await
        .map_err(|e| PrismError::BeamError(e.to_string()))?;

    let task_result_result: Result<TaskResult<RawString>, PrismError> =
        serde_json::from_str(&text).map_err(|e| PrismError::DeserializationError(e.to_string()));

    let task_result = task_result_result?;

    let decoded: Result<Vec<u8>, PrismError> = general_purpose::STANDARD
        .decode(task_result.body.into_string())
        .map_err(|e| PrismError::DecodeError(e));

    let vector = decoded?;

    let measure_report_result: Result<MeasureReport, PrismError> = serde_json::from_slice(&vector)
        .map_err(|e| PrismError::DeserializationError(e.to_string()));

    let measure_report = measure_report_result?;

    let criteria = mr::extract_criteria(measure_report)?;

    criteria_cache.cache.insert(
        task_result.from.app_name().into(),
        (criteria, std::time::SystemTime::now()),
    );

    Ok(())
}
