use actix_web::{App, HttpResponse, HttpServer, get, middleware::Logger, web};
use log::info;
use std::path::{self, PathBuf};
use std::str::FromStr;
use tokio::sync::Mutex;

mod cache;
mod templating;
const DEFAULT_ADRESS: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_MAX_CACHE: u64 = 1_000_000;

struct AppState {
    root_path: path::PathBuf,
    templates: path::PathBuf,
    cache: Mutex<cache::PageCache>,
}

struct Args<'a> {
    address: &'a str,
    port: u16,
    wwwroot: String,
    templates: String,
    cache_size: u64,
}

#[derive(Debug)]
struct ArgumentParseError {
    msg: String,
}
impl From<&str> for ArgumentParseError {
    fn from(value: &str) -> Self {
        Self {
            msg: value.to_owned(),
        }
    }
}
impl From<String> for ArgumentParseError {
    fn from(value: String) -> Self {
        Self { msg: value }
    }
}
impl std::fmt::Display for ArgumentParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Argument parse error: {}", self.msg)
    }
}
impl std::error::Error for ArgumentParseError {}

fn parse_cache_size(raw: &str) -> Result<u64, Box<dyn std::error::Error>> {
    if let Some(first_char) = raw.find(|c: char| c.is_alphabetic()) {
        let num_part = &raw[..first_char];
        if num_part.is_empty() {
            return Err(Box::new(ArgumentParseError::from(
                "Cache size must begin with a number",
            )));
        }
        let size = num_part.parse::<u64>()?;
        let unit_part = &raw[first_char..];
        let unit = match unit_part {
            "K" | "k" => Ok(1_000u64),
            "M" | "m" => Ok(1_000_000u64),
            "G" | "g" => Ok(1_000_000_000u64),
            _ => Err(ArgumentParseError::from(format!(
                "Unknown cache size unit `{unit_part}`"
            ))),
        }?;
        Ok(size * unit)
    } else {
        let size = raw.parse::<u64>()?;
        Ok(size)
    }
}

fn parse_args<'a>() -> Args<'a> {
    let args = clap::Command::new("groon-server")
        .arg(
            clap::Arg::new("address")
                .env("GROONADDRESS")
                .short('a')
                .long("address"),
        )
        .arg(
            clap::Arg::new("port")
                .env("GROONPORT")
                .short('p')
                .long("port"),
        )
        .arg(
            clap::Arg::new("templates")
                .env("GROONTEMPLATES")
                .short('t')
                .long("templates-dir")
                .required(true),
        )
        .arg(
            clap::Arg::new("wwwroot")
                .env("GROONWWWROOT")
                .short('w')
                .long("wwwroot-dir")
                .required(true),
        )
        .arg(
            clap::Arg::new("max-cache")
                .env("GROONMAXCACHE")
                .long("max-cache")
                .required(false),
        )
        .get_matches();
    let address = args
        .get_one::<&str>("address")
        .map_or(DEFAULT_ADRESS, |a| a);
    let port = args.get_one::<u16>("port").map_or(DEFAULT_PORT, |p| *p);
    let wwwroot = args
        .get_one::<String>("wwwroot")
        .expect("wwwroot not provided")
        .to_owned();
    let templates = args
        .get_one::<String>("templates")
        .expect("templates directory not provided")
        .to_owned();
    let max_cache = args
        .get_one::<String>("max-cache")
        .map(|raw| parse_cache_size(raw))
        .map(|r| match r {
            Ok(sz) => sz,
            Err(e) => {
                log::error!("Error while parsing max cache size parameter: {e}");
                DEFAULT_MAX_CACHE
            }
        })
        .unwrap_or(DEFAULT_MAX_CACHE)
        .to_owned();
    Args {
        address,
        port,
        wwwroot,
        templates,
        cache_size: max_cache,
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = parse_args();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    let cache = cache::PageCache::new(args.cache_size);
    log::info!("Using cache of size {}", args.cache_size);
    let app_state = AppState {
        root_path: path::PathBuf::from_str(&args.wwwroot)
            .unwrap()
            .canonicalize()
            .unwrap(),
        templates: path::PathBuf::from_str(&args.templates)
            .unwrap()
            .canonicalize()
            .unwrap(),
        cache: Mutex::new(cache),
    };
    let app_state = web::Data::new(app_state);
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(Logger::new("%a ${User-Agent}i"))
            .app_data(app_state.clone())
            .service(serve_files)
    })
    .bind((args.address, args.port))?
    .run()
    .await
}

#[get("/{tail:.*}")]
async fn serve_files(
    _req: actix_web::HttpRequest,
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, templating::GroonError> {
    let mut relpath = path::PathBuf::new();
    relpath.push(state.root_path.clone());
    let Ok(path) = PathBuf::from_str(&path);
    relpath.push(path);
    info!("Requested resource: {}", relpath.to_str().unwrap());
    if !relpath.exists() {
        return Ok(HttpResponse::NotFound().body("<h1> Not Found </h1>"));
    }
    match templating::ResourcePath::try_from_path(relpath) {
        Ok(resource) => {
            let mut cache = state.cache.lock().await;
            let html = templating::process_resource(resource, &state.templates, &mut cache).await?;
            Ok(HttpResponse::Ok().body(html.content))
        }
        Err(path) => {
            let file = tokio::fs::read(path).await?;
            Ok(HttpResponse::Ok().body(file))
        }
    }
}
