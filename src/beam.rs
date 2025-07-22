use crate::config::CONFIG;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use beam_lib::{AppId, MsgId, RawString, TaskRequest};
use uuid::Uuid;

pub fn create_beam_task(target_sites: Vec<String>) -> TaskRequest<RawString> {
    let target_app = &CONFIG.target_app;
    let id = MsgId::new();
    let proxy_id = &CONFIG.beam_app_id_long.proxy_id();
    let query_encoded: String = BASE64.encode(
        CONFIG
            .query_unencoded
            .replace("{{LIBRARY_UUID}}", Uuid::new_v4().to_string().as_str())
            .replace("{{MEASURE_UUID}}", Uuid::new_v4().to_string().as_str()),
    );
    let broker_id = proxy_id
        .as_ref()
        .split_once('.')
        .expect("Invalid beam id in config")
        .1;
    let to = target_sites
        .iter()
        .map(|site| AppId::new_unchecked(format!("{target_app}.{site}.{broker_id}")))
        .collect();
    let metadata = {
        serde_json::json!({
            "project": &CONFIG.project,
            "execute": false
        })
    };
    TaskRequest {
        id,
        from: CONFIG.beam_app_id_long.clone(),
        to,
        metadata,
        body: query_encoded.into(),
        failure_strategy: beam_lib::FailureStrategy::Retry {
            backoff_millisecs: 1000,
            max_tries: 5,
        },
        ttl: "360s".to_string(),
    }
}
