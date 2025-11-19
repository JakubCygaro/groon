use super::GroonTag;
use super::errors::*;
use crate::cache;
use crate::cache::PageCache;
use log::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;
use std::time::SystemTime;

const GROON_TAG_START: &str = "<?groon ";
const COMMENT_TAG_START: &str = "<!--";
const COMMENT_TAG_END: &str = "-->";

#[derive(Clone)]
pub enum ResourcePath {
    Html(PathBuf),
    Markdown(PathBuf),
}
impl ResourcePath {
    /// try to create a ResourcePath enum with the given path, otherwise just return that path
    /// wrapped in an Err() variant
    pub fn try_from_path(path: PathBuf) -> Result<Self, PathBuf> {
        let ext = path.extension().and_then(|ex| ex.to_str());
        match ext {
            Some("html") => Ok(Self::Html(path)),
            Some("md") => Ok(Self::Markdown(path)),
            _ => Err(path),
        }
    }
    pub fn get_path(&self) -> &PathBuf {
        match self {
            ResourcePath::Html(p) => p,
            ResourcePath::Markdown(p) => p,
        }
    }
}
#[derive(Clone, Debug)]
pub struct HTMLFile {
    pub content: String,
    pub dependencies: HashSet<PathBuf>,
}

async fn is_outdated(path: &PathBuf, cache: &PageCache) -> Result<bool, GroonError> {
    let meta = tokio::fs::metadata(path.clone()).await?;
    Ok(meta.modified()? >= cache.get_page(path).map(|p| p.last_modified).unwrap())
}

pub async fn read_resource_or_load_from_cache(
    resource: ResourcePath,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
    root_deps: Option<&HashSet<PathBuf>>,
) -> Result<HTMLFile, GroonError> {
    if cache.has_page(resource.get_path()) {
        log::debug!("{:?} cache hit", resource.get_path());
        if !is_outdated(resource.get_path(), cache).await? {
            let page = cache.get_page(resource.get_path()).cloned().unwrap();
            cache.page_accessed_now(resource.get_path().to_owned());
            log::debug!("{:?} return cached", resource.get_path());
            return Ok(HTMLFile {
                content: page.contents,
                dependencies: page.dependencies,
            });
        }
    }
    log::debug!("cache miss");
    let ret = read_resource(resource.clone(), temps, cache, root_deps).await?;
    cache.update_page(resource.get_path().clone(), |p| {
        p.contents = ret.content.clone();
        p.dependencies = ret.dependencies.clone();
        p.last_modified = SystemTime::now();
        p.last_accessed = Instant::now();
    });
    Ok(HTMLFile {
        content: ret.content,
        dependencies: ret.dependencies,
    })
}
/// ## Return value:
/// Tells you whether a cache read has occured
pub async fn load_resource_to_cache(
    resource: ResourcePath,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<bool, GroonError> {
    if cache.has_page(resource.get_path()) {
        if is_outdated(resource.get_path(), cache).await? {
            let ret = read_resource(resource.clone(), temps, cache, None).await?;
            cache.update_page(resource.get_path().to_owned(), |p| {
                p.contents = ret.content.clone();
                p.dependencies = ret.dependencies.clone();
                p.last_modified = SystemTime::now();
            });
            return Ok(true);
        }
    } else {
        let ret = read_resource(resource.clone(), temps, cache, None).await?;
        cache.update_page(resource.get_path().clone(), |p| {
            p.contents = ret.content.clone();
            p.dependencies = ret.dependencies.clone();
            p.last_modified = SystemTime::now();
        });
        return Ok(true);
    }
    Ok(false)
}
pub async fn read_resource(
    resource: ResourcePath,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
    root_deps: Option<&HashSet<PathBuf>>,
) -> Result<HTMLFile, GroonError> {
    match resource {
        ResourcePath::Html(path) => read_html_file(path, temps, cache, root_deps).await,
        ResourcePath::Markdown(path) => read_markdown_file(path).await,
    }
}
pub async fn read_html_file(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
    root_deps: Option<&HashSet<PathBuf>>,
) -> Result<HTMLFile, GroonError> {
    log::debug!("{:?} read_html_file", path);
    let content = tokio::fs::read_to_string(path.clone()).await?;

    let mut dependencies: HashSet<PathBuf> = HashSet::new();
    let mut ret = String::with_capacity(content.len());
    let mut slice = &content[..];

    while let Some(idx) = slice.find(GROON_TAG_START) {
        if let Some(comment_start) = slice.find(COMMENT_TAG_START) {
            let comment_end = slice
                .find(COMMENT_TAG_END)
                .ok_or(TagParseError::UnclosedComment(path.clone()))?;
            if comment_start < idx && comment_end > idx {
                ret.push_str(&slice[..comment_start]);
                slice = &slice[comment_end + COMMENT_TAG_END.len()..];
                continue;
            }
        }
        ret.push_str(&slice[..idx]);
        slice = &slice[idx..];
        let Some(tag_end) = slice.find('>') else {
            return Err(TagParseError::PrematureEnd(path.clone()))?;
        };
        let tag = parse_groon_tag(&slice[GROON_TAG_START.len()..tag_end], &path)?;
        let tag_expand =
            expand_groon_tag(tag, &path, temps, &mut dependencies, cache, root_deps).await?;
        ret.push_str(&tag_expand.content);
        dependencies.extend(tag_expand.dependencies);
        slice = &slice[tag_end + 1..];
    }
    ret.push_str(slice);
    Ok(HTMLFile {
        content: ret,
        dependencies,
    })
}

async fn expand_groon_tag(
    tag: GroonTag,
    path: &PathBuf,
    temps: &PathBuf,
    dependencies: &mut HashSet<PathBuf>,
    cache: &mut cache::PageCache,
    root_deps: Option<&HashSet<PathBuf>>,
) -> Result<HTMLFile, GroonError> {
    let tag_expand = match tag {
        GroonTag::Insert(template_path) => {
            if template_path.get_path().file_name() == path.file_name() {
                warn!(
                    "Self referential template {:?}",
                    template_path.get_path()
                );
                return Err(GroonError::TagProcessing(
                    TagProcessingError::SelfRefelercial(template_path.get_path().to_owned()),
                ));
            }
            let temp_path = temps.join(template_path.get_path());
            // this technically should not fail ever
            let temp_path = ResourcePath::try_from_path(temp_path).unwrap();
            dependencies.insert(temp_path.get_path().clone());
            // if no root deps were provided, use the current file dependencies as root
            let root_deps = match root_deps {
                Some(rd) => {
                    log::debug!("rd: {:?}", rd);
                    log::debug!("temp_path: {:?}", temp_path.get_path());
                    rd.contains(temp_path.get_path())
                        .then(|| {
                            GroonError::TagProcessing(TagProcessingError::DependencyCycle {
                                file: path.to_owned(),
                                dep: temp_path.get_path().to_owned(),
                            })
                        })
                        .map_or_else(|| Ok(()), Err)?;
                    Some(rd)
                }
                None => Some(&(*dependencies)),
            };
            Box::pin(read_resource_or_load_from_cache(
                temp_path,
                temps,
                cache,
                root_deps,
            ))
            .await?
        }
    };
    Ok(tag_expand)
}

pub async fn read_markdown_file(path: PathBuf) -> Result<HTMLFile, GroonError> {
    let md = tokio::fs::read_to_string(path).await?;
    let content = markdown::to_html_with_options(&md, &markdown::Options::gfm()).unwrap();
    Ok(HTMLFile {
        content,
        dependencies: HashSet::new(),
    })
}

pub fn parse_groon_tag(tag_str: &str, file: &PathBuf) -> Result<GroonTag, TagParseError> {
    let mut spl = tag_str.split('=');
    let Some(kwd) = spl.next() else {
        return Err(TagParseError::EmptyTag(file.to_owned()));
    };
    match kwd {
        "insert" => {
            let path = spl.next().ok_or(TagParseError::MissingValue {
                file: file.to_owned(),
                attr: kwd.to_string(),
            })?;
            if !path.starts_with('"') || !path.ends_with('"') {
                return Err(TagParseError::UnquotedValue {
                    file: file.to_owned(),
                    attr: kwd.to_string(),
                });
            }
            let path = &path[1..path.len() - 1];
            let insert = PathBuf::from_str(path).unwrap();
            let insert = ResourcePath::try_from_path(insert)
                .map_err(|p|{
                    TagParseError::InvalidInsertFileType { file: p }
            })?;
            Ok(GroonTag::Insert(insert))
        }
        _ => Err(TagParseError::Unrecognized {
            file: file.to_owned(),
            tag: kwd.to_string(),
        }),
    }
}
