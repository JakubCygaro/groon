use actix_web::{App, HttpResponse, HttpServer, Responder, get, middleware::Logger, web};
use futures::stream::{self, StreamExt};
use log::{debug, info, warn};
use std::path::{self, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;
use tokio::sync::Mutex;

use self::cache::PageInfo;
mod cache;
mod templating;
const DEFAULT_ADRESS: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8080;

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
    Args {
        address,
        port,
        wwwroot,
        templates,
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = parse_args();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    let cache = cache::PageCache::new();
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(Logger::new("%a ${User-Agent}i"))
            .app_data(web::Data::new(AppState {
                root_path: path::PathBuf::from_str(&args.wwwroot)
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
                templates: path::PathBuf::from_str(&args.templates)
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
                cache: Mutex::new(cache.clone())
            }))
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
    match relpath.extension().and_then(|ex| ex.to_str()) {
        Some("html") => {
            let mut cache = state.cache.lock().await;
            let tmp = templating::process_html_file(relpath.clone(), &state.templates, &mut cache).await?;
            Ok(HttpResponse::Ok().body(tmp.content))
        }
        Some("md") => {
            let mut cache = state.cache.lock().await;
            let tmp = templating::process_markdown_file(state.templates.join(relpath), &mut cache).await?;
            Ok(HttpResponse::Ok().body(tmp))
        }
        _ => {
            let file = tokio::fs::read(relpath).await?;
            Ok(HttpResponse::Ok().body(file))
        }
    }
}
