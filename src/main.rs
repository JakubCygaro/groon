use actix_web::{App, HttpResponse, HttpServer, Responder, get, middleware::Logger, web};
use log::{debug, info, warn};
use std::path::{self, PathBuf};
use std::str::FromStr;

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
) -> impl Responder {
    let mut relpath = path::PathBuf::new();
    relpath.push(state.root_path.clone());
    let Ok(path) = PathBuf::from_str(&path);
    relpath.push(path);
    debug!("{}", relpath.to_str().unwrap());
    if !relpath.exists() {
        return HttpResponse::NotFound().body("<h1> Not Found </h1>");
    }
    match relpath.extension().and_then(|ex| ex.to_str()) {
        Some("html") => match process_html_file(relpath, &state.templates).await {
            Ok(html) => HttpResponse::Ok().body(html),
            Err(msg) => HttpResponse::InternalServerError().body(msg),
        },
        _ => {
            let Ok(file) = tokio::fs::read(relpath).await else {
                return HttpResponse::NotFound().body("<h1> Not Found </h1>");
            };
            HttpResponse::Ok().body(file)
        }
    }
}

async fn process_html_file(path: PathBuf, temps: &PathBuf) -> Result<String, &'static str> {
    const GROON_TAG_START: &str = "<?groon ";
    let content = tokio::fs::read_to_string(path.clone())
        .await
        .map_err(|_| "read error")?;
    let mut ret = String::with_capacity(content.len());
    let mut slice = &content[..];
    while let Some(idx) = slice.find(GROON_TAG_START) {
        ret.push_str(&slice[..idx]);
        slice = &slice[idx..];
        let Some(tag_end) = slice.find('>') else {
            return Err("premature end of input");
        };
        let Ok(tag) = parse_groon_tag(&slice[GROON_TAG_START.len()..tag_end]) else {
            return Err("unrecognized groon tag type");
        };
        let tag_expand = match tag {
            GroonTag::Insert(template_path) => {
                if template_path.file_name() == path.file_name() {
                    warn!(
                        "Self referential template {}",
                        template_path.to_str().unwrap_or("")
                    );
                    return Ok("".to_string());
                }
                match template_path.extension().and_then(|ex| ex.to_str()) {
                    Some("html") => {
                        Box::pin(process_html_file(temps.join(template_path), temps)).await?
                    }
                    Some("md") => {
                        let md = tokio::fs::read_to_string(temps.join(template_path))
                            .await
                            .map_err(|_| "could not read markdown")?;
                        markdown::to_html_with_options(&md, &markdown::Options::gfm()).unwrap()
                    }
                    _ => {
                        warn!(
                            "Invalid insert template file type"
                        );
                        return Ok("".to_string())
                    }
                }
            }
        };
        ret.push_str(&tag_expand);
        slice = &slice[tag_end + 1..];
    }
    ret.push_str(slice);
    Ok(ret)
}
enum GroonTag {
    Insert(PathBuf),
}
fn parse_groon_tag(tag_str: &str) -> Result<GroonTag, &'static str> {
    let mut spl = tag_str.split('=');
    let Some(kwd) = spl.next() else {
        return Err("could not parse groon tag");
    };
    match kwd {
        "insert" => {
            let path = spl.next().ok_or("invalid tag")?;
            if !path.starts_with('"') || !path.ends_with('"') {
                return Err("unqoted import attribute parameter");
            }
            let path = &path[1..path.len() - 1];
            Ok(GroonTag::Insert(PathBuf::from_str(path).unwrap()))
        }
        _ => Err("unknown tag type"),
    }
}
