use actix_web::ResponseError;
use actix_web::{App, HttpResponse, HttpServer, Responder, get, middleware::Logger, web};
use log::{debug, info, warn};
use std::path::{self, PathBuf};
use std::str::FromStr;
mod templating;

struct AppState {
    root_path: path::PathBuf,
    templates: path::PathBuf,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let wwwroot = std::env::var("WWWROOT").expect("set WWWROOT");
    let templates = std::env::var("TEMPLATES").expect("set TEMPLATES");
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .wrap(Logger::new("%a ${User-Agent}i"))
            .app_data(web::Data::new(AppState {
                root_path: path::PathBuf::from_str(&wwwroot)
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
                templates: path::PathBuf::from_str(&templates)
                    .unwrap()
                    .canonicalize()
                    .unwrap(),
            }))
            .service(serve_files)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[get("/{tail:.*}")]
async fn serve_files(
    _req: actix_web::HttpRequest,
    path: web::Path<String>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, templating::GroonError>{
    let mut relpath = path::PathBuf::new();
    relpath.push(state.root_path.clone());
    let Ok(path) = PathBuf::from_str(&path);
    relpath.push(path);
    debug!("{}", relpath.to_str().unwrap());
    if !relpath.exists() {
        return Ok(HttpResponse::NotFound().body("<h1> Not Found </h1>"));
    }
    match relpath.extension().and_then(|ex| ex.to_str()) {
        Some("html") => {
            let tmp = templating::process_html_file(relpath, &state.templates).await?;
            Ok(HttpResponse::Ok().body(tmp))
        },
        _ => {
            let file = tokio::fs::read(relpath).await?;
            Ok(HttpResponse::Ok().body(file))
        }
    }
}

