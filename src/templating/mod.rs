use futures::stream::{self, StreamExt};
use log::warn;
use std::path::PathBuf;
mod errors;
mod parse;
use crate::cache;
pub use errors::GroonError;
pub use parse::HTMLFile;
use std::time::SystemTime;

pub enum GroonTag {
    Insert(PathBuf),
}

pub async fn process_html_file(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    if let Some(deps) = cache.get_page(&path).map(|p| {
        log::debug!("dep size {}", p.dependencies.len());
        p.dependencies.clone()
    }) {
        log::debug!("{:?} with_deps", path);
        return process_html_with_deps(path, deps, temps, cache).await;
    } else {
        log::debug!("{:?} flat", path);
        let page = parse::read_html_or_load_from_cache(path.clone(), temps, cache).await?;
        Ok(page)
    }
}
async fn process_html_with_deps(
    path: PathBuf,
    deps: Vec<PathBuf>,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    let mut should_reread = false;
    for dep_path in &deps {
        log::debug!("dep: {:?}", dep_path);
        should_reread |= process_html_or_markdown_file(dep_path.clone(), temps, cache).await?;
    }
    let page = if should_reread {
        log::debug!("{:?} reread", path);
        let read = parse::read_html_file(path.clone(), temps, cache).await?;
        cache.update_page(path, |p|{
            p.contents = read.content.clone();
            p.dependencies = read.dependencies.clone();
            p.last_modified = SystemTime::now();
        });
        read
    } else {
        log::debug!("{:?} load from cache", path);
        parse::read_html_or_load_from_cache(path.clone(), temps, cache).await?
    };
    Ok(page)
}
async fn process_html_or_markdown_file(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<bool, GroonError> {
    match path.extension().and_then(|ex| ex.to_str()) {
        Some("html") => parse::load_html_to_cache(path, temps, cache).await,
        Some("md") => parse::load_markdown_to_cache(path, cache).await,
        _ => unreachable!(),
    }
}
pub async fn process_markdown_file(
    path: PathBuf,
    cache: &mut cache::PageCache,
) -> Result<String, GroonError> {
    let md = tokio::fs::read_to_string(&path).await?;
    Ok(markdown::to_html_with_options(&md, &markdown::Options::gfm()).unwrap())
}
