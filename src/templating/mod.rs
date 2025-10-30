use std::path::PathBuf;
use std::str::FromStr;
use log::{warn};
mod errors;
pub use errors::{GroonError, TagParseError};
pub enum GroonTag {
    Insert(PathBuf)
}
pub async fn process_html_file(path: PathBuf, temps: &PathBuf) -> Result<String, GroonError> {
    const GROON_TAG_START: &str = "<?groon ";
    let content = tokio::fs::read_to_string(path.clone())
        .await?;
    let mut ret = String::with_capacity(content.len());
    let mut slice = &content[..];
    while let Some(idx) = slice.find(GROON_TAG_START) {
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
                    return Ok("".to_string());
                }
                match template_path.extension().and_then(|ex| ex.to_str()) {
                    Some("html") => {
                        Box::pin(process_html_file(temps.join(template_path), temps)).await?
                    }
                    Some("md") => {
                        let md = tokio::fs::read_to_string(temps.join(template_path))
                            .await?;
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
fn parse_groon_tag(tag_str: &str) -> Result<GroonTag, TagParseError> {
    let mut spl = tag_str.split('=');
    let Some(kwd) = spl.next() else {
        return Err(TagParseError::EmptyTag);
    };
    match kwd {
        "insert" => {
            let path = spl.next().ok_or(TagParseError::MissingValue { attr: kwd.to_string() })?;
            if !path.starts_with('"') || !path.ends_with('"') {
                return Err(TagParseError::UnquotedValue { attr: kwd.to_string() });
            }
            let path = &path[1..path.len() - 1];
            Ok(GroonTag::Insert(PathBuf::from_str(path).unwrap()))
        }
        _ => Err(TagParseError::Unrecognized(kwd.to_string())),
    }
}
