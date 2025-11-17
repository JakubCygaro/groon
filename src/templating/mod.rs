use std::collections::HashSet;
use std::path::PathBuf;
mod errors;
mod parse;
use crate::cache;
pub use errors::GroonError;
pub use parse::{HTMLFile, ResourcePath};
use std::time::SystemTime;

pub enum GroonTag {
    Insert(PathBuf),
}

pub async fn process_resource(
    resource: ResourcePath,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    if let Some(deps) = cache.get_page(resource.get_path()).map(|p| {
        log::debug!("dep size {}", p.dependencies.len());
        p.dependencies.clone()
    }) {
        log::debug!("{:?} with_deps", resource.get_path());
        return process_resource_with_deps(resource, deps, temps, cache).await;
    } else {
        let page = parse::read_resource_or_load_from_cache(resource, temps, cache, None).await?;
        Ok(page)
    }
}
async fn process_resource_with_deps(
    resource: ResourcePath,
    deps: HashSet<PathBuf>,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    let mut should_reread = false;
    for dep_path in &deps {
        log::debug!("dep: {:?}", dep_path);
        let dep_as_resource =
            ResourcePath::try_from_path(dep_path.clone()).unwrap_or_else(|p|
                panic!("Cached dependency file of invalid format. File: {:?}", p)
            );
        should_reread |= parse::load_resource_to_cache(dep_as_resource, temps, cache).await?;
    }
    let page = if should_reread {
        log::debug!("{:?} reread", resource.get_path());
        let read = parse::read_resource(resource.clone(), temps, cache, None).await?;
        cache.update_page(resource.get_path().to_owned(), |p| {
            p.contents = read.content.clone().into();
            p.dependencies = read.dependencies.clone();
            p.last_modified = SystemTime::now();
            p.last_accessed = p.last_modified;
        });
        read
    } else {
        log::debug!("{:?} load from cache", resource.get_path());
        parse::read_resource_or_load_from_cache(resource, temps, cache, None).await?
    };
    Ok(page)
}
