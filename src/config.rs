use std::fs;

use beam_lib::AppId;
use clap::Parser;
use once_cell::sync::Lazy;
use tracing::{debug, info};

use std::net::SocketAddr;

use reqwest::Url;
use tower_http::cors::AllowOrigin;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::errors::PrismError;

pub(crate) static CONFIG: Lazy<Config> = Lazy::new(|| {
    debug!("Loading config");
    Config::load().unwrap_or_else(|e| {
        eprintln!("Unable to start as there was an error reading the config:\n{}\n\nTerminating -- please double-check your startup parameters with --help and refer to the documentation.", e);
        std::process::exit(1);
    })
});

const CLAP_FOOTER: &str =
    "For updates and detailed usage instructions, visit https://github.com/samply/prism";

#[derive(Parser, Debug)]
#[clap(
    name("🏳️‍🌈⃤  Prism"),
    version,
    arg_required_else_help(true),
    after_help(CLAP_FOOTER)
)]
struct CliArgs {
    /// The beam proxy's base URL, e.g. https://proxy1.broker.samply.de
    #[clap(long, env, value_parser)]
    beam_proxy_url: Url,

    /// This application's beam AppId, e.g. prism.proxy1.broker.samply.de
    #[clap(long, env, value_parser)]
    beam_app_id_long: String,

    /// This application's beam API key
    #[clap(long, env, value_parser)]
    api_key: String,

    /// Sites to initially query, separated by ';'
    #[clap(long, env, value_parser)]
    sites: String,

    /// Where to allow cross-origin resourse sharing from
    #[clap(long, env, value_parser = parse_cors)]
    pub cors_origin: AllowOrigin,

    /// Project name
    #[clap(long, env)]
    pub project: String,

    /// The socket address this server will bind to
    #[clap(long, env, default_value = "0.0.0.0:8080")]
    pub bind_addr: SocketAddr,

    /// Target_application_name
    #[clap(long, env, value_parser, default_value = "focus")]
    target: String,
}

#[derive(Debug)]
pub(crate) struct Config {
    pub beam_proxy_url: Url,
    pub beam_app_id_long: AppId,
    pub api_key: String,
    pub sites: Vec<String>,
    pub cors_origin: AllowOrigin,
    pub project: String,
    pub bind_addr: SocketAddr,
    pub query_unencoded: String,
    pub target: String,
}

impl Config {
    fn load() -> Result<Self, PrismError> {
        let cli_args = CliArgs::parse();
        info!("Successfully read config and API keys from CLI and secrets files.");
        let config = Config {
            beam_proxy_url: cli_args.beam_proxy_url,
            beam_app_id_long: AppId::new_unchecked(cli_args.beam_app_id_long),
            api_key: cli_args.api_key,
            sites: cli_args.sites.split(';').map(|s| s.to_string()).collect(),
            cors_origin: cli_args.cors_origin,
            project: cli_args.project,
            bind_addr: cli_args.bind_addr,
            query_unencoded: get_query_unencoded(),
            target: cli_args.target,
        };
        Ok(config)
    }
}

fn get_query_unencoded() -> String {
    let cql_file_name: String = format!("resources/query_{}.cql", CliArgs::parse().project);
    let body_file_name = format!("resources/body_{}.json", CliArgs::parse().project);

    fs::read_to_string(&body_file_name)
        .unwrap_or_else(|_| panic!("File {} can't be read", &body_file_name))
        .replace(
            "{{LIBRARY_ENCODED}}",
            BASE64
                .encode(
                    fs::read_to_string(&cql_file_name)
                        .unwrap_or_else(|_| panic!("File {} can't be read", &cql_file_name))
                        .as_str(),
                )
                .as_str(),
        )
}

fn parse_cors(v: &str) -> Result<AllowOrigin, reqwest::header::InvalidHeaderValue> {
    if v == "*" || v.to_lowercase() == "any" {
        Ok(AllowOrigin::any())
    } else {
        v.parse().map(AllowOrigin::exact)
    }
}
