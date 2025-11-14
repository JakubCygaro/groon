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
    if let Some(deps) = cache.get_page(&path).map(|p| p.dependencies.clone()) {
        return process_html_with_deps(path, deps, temps, cache).await;
    } else {
        let page = parse::read_html_file(path.clone(), &temps, cache).await?;
        cache.update_page(path, |p| {
            p.contents = page.content.clone();
            p.dependencies = page.dependencies.clone();
            p.last_modified = SystemTime::now();
        });
        Ok(page)
    }
}
async fn process_html_with_deps(
    path: PathBuf,
    deps: Vec<PathBuf>,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    let meta = tokio::fs::metadata(path.clone()).await?;
    let mut reload_file = meta.modified().unwrap_or(SystemTime::now())
        >= cache
            .get_page(&path)
            .map(|p| p.last_modified)
            .unwrap_or(SystemTime::now());

    for dep_path in &deps {
        match cache.get_page(dep_path).map(|p| p.last_modified) {
            Some(dep_last_modified) => {
                let meta = tokio::fs::metadata(dep_path).await?;
                // dep has been modified after last access
                if meta.modified().unwrap_or(SystemTime::now()) >= dep_last_modified {
                    let page =
                        process_html_or_markdown_file(dep_path.clone(), temps, cache).await?;
                    cache.update_page(dep_path.clone(), |p| {
                        p.last_modified = SystemTime::now();
                        p.contents = page.content.clone();
                        p.dependencies = page.dependencies.clone();
                    });
                    reload_file = true;
                }
            }
            None => {
                let html_file = parse::read_html_file(dep_path.clone(), temps, cache).await?;
                cache.add_page(dep_path.to_owned(), html_file.into());
            }
        };
    }
    if reload_file {
        let page = parse::read_html_file(path.clone(), temps, cache).await?;
        cache.update_page(path, |p| {
            p.last_modified = SystemTime::now();
            p.dependencies = page.dependencies.clone();
            p.contents = page.content.clone();
        });
        Ok(page)
    } else {
        let page = cache.get_page(&path).cloned().unwrap();
        Ok(HTMLFile {
            content: page.contents,
            dependencies: page.dependencies,
        })
    }
}
async fn process_html_or_markdown_file(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    match path.extension().and_then(|ex| ex.to_str()) {
        Some("html") => parse::read_html_file(path, temps, cache).await,
        Some("md") => parse::read_markdown_file(path, cache).await,
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
