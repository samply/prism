use crate::config::CONFIG;
use beam_lib::{AppId, MsgId, RawString, TaskRequest};

pub fn create_beam_task(target_sites: &[impl ToString]) -> TaskRequest<RawString> {
    let id = MsgId::new();
    let proxy_id = &CONFIG.beam_app_id_long.proxy_id();
    let broker_id = proxy_id
        .as_ref()
        .split_once('.')
        .expect("Invalid beam id in config")
        .1;
    let to = target_sites
        .iter()
        .map(|site| {
            let site = site.to_string();
            AppId::new_unchecked(format!("focus.{site}.{broker_id}"))
        })
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
        body: CONFIG.query.clone().into(),
        failure_strategy: beam_lib::FailureStrategy::Retry {
            backoff_millisecs: 1000,
            max_tries: 5,
        },
        ttl: "360s".to_string(),
    }
}
