use super::GroonTag;
use super::errors::*;
use crate::cache;
use crate::cache::PageCache;
use log::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

const GROON_TAG_START: &str = "<?groon ";
const COMMENT_TAG_START: &str = "<!--";
const COMMENT_TAG_END: &str = "-->";

#[derive(Clone, Debug)]
pub struct HTMLFile {
    pub content: String,
    pub dependencies: HashSet<PathBuf>,
}

async fn is_outdated(path: &PathBuf, cache: &PageCache) -> Result<bool, GroonError> {
    let meta = tokio::fs::metadata(path.clone()).await?;
    Ok(meta.modified()? >= cache.get_page(path).map(|p| p.last_modified).unwrap())
}

pub async fn read_page_or_load_from_cache(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
    root_deps: Option<&HashSet<PathBuf>>,
) -> Result<HTMLFile, GroonError> {

    todo!()
}

pub async fn read_html_or_load_from_cache(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
    root_deps: Option<&HashSet<PathBuf>>,
) -> Result<HTMLFile, GroonError> {
    if cache.has_page(&path) {
        log::debug!("{:?} cache hit", path);
        if !is_outdated(&path, cache).await? {
            let page = cache.get_page(&path).cloned().unwrap();
            log::debug!("{:?} return cached", path);
            return Ok(HTMLFile {
                content: page.contents.expect("cache invalidated"),
                dependencies: page.dependencies,
            });
        }
    }
    log::debug!("cache miss");
    let ret = read_html_file(path.clone(), temps, cache, root_deps).await?;
    cache.update_page(path, |p| {
        p.contents = ret.content.clone().into();
        p.dependencies = ret.dependencies.clone();
        p.last_modified = SystemTime::now();
        p.last_accessed = p.last_modified;
    });
    Ok(HTMLFile {
        content: ret.content,
        dependencies: ret.dependencies,
    })
}

pub async fn load_html_to_cache(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<bool, GroonError> {
    if cache.has_page(&path) {
        if is_outdated(&path, cache).await? {
            let ret = read_html_file(path.clone(), temps, cache, None).await?;
            cache.update_page(path, |p| {
                p.contents = ret.content.clone().into();
                p.dependencies = ret.dependencies.clone();
                p.last_modified = SystemTime::now();
            });
            return Ok(true);
        }
    } else {
        let ret = read_html_file(path.clone(), temps, cache, None).await?;
        cache.update_page(path, |p| {
            p.contents = ret.content.clone().into();
            p.dependencies = ret.dependencies.clone();
            p.last_modified = SystemTime::now();
        });
        return Ok(true);
    }
    Ok(false)
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
            if template_path.file_name() == path.file_name() {
                warn!(
                    "Self referential template {}",
                    template_path.to_str().unwrap_or("")
                );
                return Err(GroonError::TagProcessing(
                    TagProcessingError::SelfRefelercial(template_path),
                ));
            }
            let temp_path = temps.join(&template_path);
            dependencies.insert(temp_path.clone());
            // if no root deps were provided, use the current file dependencies as root
            let root_deps = match root_deps {
                Some(rd) => {
                    log::debug!("rd: {:?}", rd);
                    log::debug!("temp_path: {:?}", temp_path);
                    rd.contains(&temp_path)
                        .then(|| {
                            GroonError::TagProcessing(TagProcessingError::DependencyCycle {
                                file: path.to_owned(),
                                dep: temp_path.to_owned(),
                            })
                        })
                        .map_or_else(|| Ok(()), Err)?;
                    Some(rd)
                }
                None => Some(&(*dependencies)),
            };
            match template_path.extension().and_then(|ex| ex.to_str()) {
                Some("html") => {
                    Box::pin(read_html_or_load_from_cache(
                        temp_path, temps, cache, root_deps,
                    ))
                    .await?
                }
                Some("md") => read_markdown_or_load_from_cache(temp_path, cache).await?,
                _ => {
                    return Err(GroonError::TagProcessing(
                        TagProcessingError::SelfRefelercial(template_path),
                    ));
                }
            }
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

pub async fn read_markdown_or_load_from_cache(
    path: PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    if cache.has_page(&path) && !is_outdated(&path, cache).await? {
        let page = cache.get_page(&path).cloned().unwrap();
        cache.page_accessed_now(path);
        return Ok(HTMLFile {
            content: page.contents.expect("cache invalidated"),
            dependencies: page.dependencies,
        });
    }
    let ret = read_markdown_file(path.clone()).await?;
    cache.update_page(path, |p| {
        p.contents = ret.content.clone().into();
        p.dependencies = ret.dependencies.clone();
        p.last_modified = SystemTime::now();
        p.last_accessed = p.last_modified;
    });
    Ok(HTMLFile {
        content: ret.content,
        dependencies: ret.dependencies,
    })
}
pub async fn load_markdown_to_cache(
    path: PathBuf,
    cache: &mut cache::PageCache,
) -> Result<bool, GroonError> {
    if cache.has_page(&path) {
        if is_outdated(&path, cache).await? {
            let ret = read_markdown_file(path.clone()).await?;
            cache.update_page(path, |p| {
                p.contents = ret.content.clone().into();
                p.dependencies = ret.dependencies.clone();
                p.last_modified = SystemTime::now();
            });
            return Ok(true);
        }
    } else {
        let ret = read_markdown_file(path.clone()).await?;
        cache.update_page(path, |p| {
            p.contents = ret.content.clone().into();
            p.dependencies = ret.dependencies.clone();
            p.last_modified = SystemTime::now();
        });
        return Ok(true);
    }
    Ok(false)
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
            if let Some(ext) = insert.extension().and_then(|ex| ex.to_str())
                && (ext == "html" || ext == "md")
            {
                return Ok(GroonTag::Insert(insert));
            }
            Err(TagParseError::InvalidInsertFileType { file: insert })
        }
        _ => Err(TagParseError::Unrecognized {
            file: file.to_owned(),
            tag: kwd.to_string(),
        }),
    }
}
