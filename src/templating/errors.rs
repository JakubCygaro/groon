use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use actix_web::http::header::ContentType;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GroonError {
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("Tag parsing error")]
    TagParse(#[from] TagParseError),
    #[error("Tag processing error")]
    TagProcessing(#[from] TagProcessingError),
}

fn get_groon_error_html(error_msg: &str, source: &str) -> String {
    format!(
        r#"
    <!DOCTYPE html>
    <html>
        <head>
            <meta charset="utf-8"></meta>
            <title>Groon error</title>
            <style>
                .error-box {{
                    margin: auto;
                    width: 50%;
                    text-align: center;
                }}
            </style>
        </head>
        <body>
            <div class="error-box">
                <h1>Groon error</h1>
                <p>{error_msg}</p>
                <p>{source}</p>
            </div>
        </body>
    </html>
    "#
    )
}

impl actix_web::error::ResponseError for GroonError {
    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        let source = self
            .source()
            .map(|src| src.to_string())
            .unwrap_or("".to_owned());
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(get_groon_error_html(&self.to_string(), &source))
    }
    fn status_code(&self) -> actix_web::http::StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
#[derive(Debug, Error)]
pub enum TagParseError {
    #[error("Groon tag was empty. File {0}")]
    EmptyTag(PathBuf),
    #[error("No value for attribute `{attr:?}`. File: {file:?}")]
    MissingValue { file: PathBuf, attr: String },
    #[error("Unquoted value for attribute `{attr:?}`. File: {file:?}")]
    UnquotedValue { file: PathBuf, attr: String },
    #[error("Unrecognized tag `{tag:?}`. File: {file:?}")]
    Unrecognized { file: PathBuf, tag: String },
    #[error("Invalid insert template file type. File: {file:?}")]
    InvalidInsertFileType{ file: PathBuf },
    #[error("Unclosed comment. File {0}")]
    UnclosedComment(PathBuf),
    #[error("Premature end of input. File {0}")]
    PrematureEnd(PathBuf),
}
#[derive(Debug, Error)]
pub enum TagProcessingError {
    #[error("Self referential template. File: {0} ")]
    SelfRefelercial(PathBuf),
    #[error("Dependency cycle found between {file:?} and {dep:?}")]
    DependencyCycle{ file: PathBuf, dep: PathBuf },
}
