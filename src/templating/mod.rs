use log::warn;
use std::path::PathBuf;
use std::str::FromStr;
use futures::stream::{self, StreamExt};
mod errors;
pub use errors::{GroonError, TagParseError};
use std::time::SystemTime;

use crate::cache;

const GROON_TAG_START: &str = "<?groon ";
const COMMENT_TAG_START: &str = "<!--";
const COMMENT_TAG_END: &str = "-->";

pub enum GroonTag {
    Insert(PathBuf),
}

pub struct HTMLFile {
    pub content: String,
    dependencies: Vec<PathBuf>,
}

pub async fn process_html_file(
    path: PathBuf,
    temps: &PathBuf,
    cache: &mut cache::PageCache,
) -> Result<HTMLFile, GroonError> {
    if let Some(deps) = cache.get_page(&path).map(|p| p.dependencies.clone()) {
        return process_html_with_deps(deps, cache).await;
    }
    let content = tokio::fs::read_to_string(path.clone()).await?;
    let mut dependencies: Vec<PathBuf> = vec![];
    let mut ret = String::with_capacity(content.len());
    let mut slice = &content[..];
    while let Some(idx) = slice.find(GROON_TAG_START) {
        if let Some(comment_start) = slice.find(COMMENT_TAG_START) {
            let comment_end = slice
                .find(COMMENT_TAG_END)
                .ok_or(GroonError::UnclosedComment)?;
            if comment_start < idx && comment_end > idx {
                ret.push_str(&slice[..comment_start]);
                slice = &slice[comment_end + COMMENT_TAG_END.len()..];
                continue;
            }
        }
        ret.push_str(&slice[..idx]);
        slice = &slice[idx..];
        let Some(tag_end) = slice.find('>') else {
            return Err(GroonError::PrematureEnd);
        };
        let tag = parse_groon_tag(&slice[GROON_TAG_START.len()..tag_end])?;
        let tag_expand = match tag {
            GroonTag::Insert(template_path) => {
                if template_path.file_name() == path.file_name() {
                    warn!(
                        "Self referential template {}",
                        template_path.to_str().unwrap_or("")
                    );
                    return Ok(HTMLFile {
                        content: "".to_owned(),
                        dependencies: vec![],
                    });
                }
                match template_path.extension().and_then(|ex| ex.to_str()) {
                    Some("html") => {
                        let ret =
                            Box::pin(process_html_file(temps.join(&template_path), temps, cache))
                                .await?;
                        dependencies.push(template_path.clone());
                        ret
                    }
                    Some("md") => {
                        let content = process_markdown_file(temps.join(&template_path)).await?;
                        dependencies.push(template_path.clone());
                        HTMLFile {
                            content,
                            dependencies: vec![],
                        }
                    }
                    _ => {
                        warn!("Invalid insert template file type");
                        return Ok(HTMLFile {
                            content: "".to_owned(),
                            dependencies: vec![],
                        });
                    }
                }
            }
        };
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
async fn process_html_with_deps(deps: Vec<PathBuf>, temps: &PathBuf, cache: &mut cache::PageCache) -> Result<HTMLFile, GroonError> {
        let deps_mod_time = stream::iter(deps
            .into_iter())
            .map(|d| async move {
                let meta = tokio::fs::metadata(&d).await?;
                let modified_time = match meta.modified() {
                    Ok(m) => m,
                    Err(e) => {
                        log::error!("{e}");
                        SystemTime::now()
                    }
                };
                std::io::Result::Ok((d, modified_time))
            })
            .filter_map(|r| async move {
                match r.await {
                    Ok(dmt) => Some(dmt),
                    _ => None
                }
            })
            .collect::<Vec<_>>().await;
        for (dep_path, mt) in deps_mod_time {
            let page_last_access = cache
                .get_page(&dep_path)
                .map(|p| p.last_access)
                .unwrap_or(mt);
            if page_last_access <= mt {
                let tmp = match dep_path.extension().and_then(|ex| ex.to_str()) {
                    Some("html") => {
                        let ret =
                            Box::pin(process_html_file(temps.join(&dep_path), temps, cache))
                                .await?;
                        ret
                    }
                    Some("md") => {
                        let content = process_markdown_file(temps.join(&dep_path)).await?;
                        HTMLFile{
                            content,
                            dependencies: vec![],
                        }
                    }
                    _ => {
                        warn!("Invalid insert template file type");
                        return Ok(HTMLFile {
                            content: "".to_owned(),
                            dependencies: vec![],
                        });
                    }
                };
                cache.update_page(dep_path.clone(), |p| {
                    p.last_access = SystemTime::now();
                    p.contents = tmp.content.clone();
                    p.dependencies= tmp.dependencies.clone();
                });
                return Ok(HTMLFile {
                    content: tmp.content,
                    dependencies: tmp.dependencies
                });
            } else {
                todo!()
            }
        }

}
pub async fn process_markdown_file(path: PathBuf) -> Result<String, GroonError> {
    let md = tokio::fs::read_to_string(path).await?;
    Ok(markdown::to_html_with_options(&md, &markdown::Options::gfm()).unwrap())
}
fn parse_groon_tag(tag_str: &str) -> Result<GroonTag, TagParseError> {
    let mut spl = tag_str.split('=');
    let Some(kwd) = spl.next() else {
        return Err(TagParseError::EmptyTag);
    };
    match kwd {
        "insert" => {
            let path = spl.next().ok_or(TagParseError::MissingValue {
                attr: kwd.to_string(),
            })?;
            if !path.starts_with('"') || !path.ends_with('"') {
                return Err(TagParseError::UnquotedValue {
                    attr: kwd.to_string(),
                });
            }
            let path = &path[1..path.len() - 1];
            Ok(GroonTag::Insert(PathBuf::from_str(path).unwrap()))
        }
        _ => Err(TagParseError::Unrecognized(kwd.to_string())),
    }
}
