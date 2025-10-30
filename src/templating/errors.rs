use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use thiserror::Error;
use std::io;

#[derive(Debug, Error)]
pub enum GroonError {
    #[error("IO error")]
    IO(#[from] io::Error),
    #[error("Premature end of input")]
    PrematureEnd,
    #[error("Tag parsing error")]
    TagParse(#[from] TagParseError),
}
impl actix_web::error::ResponseError for GroonError {
    fn error_response(&self) -> actix_web::HttpResponse<actix_web::body::BoxBody> {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::html())
            .body(self.to_string())
    }
    fn status_code(&self) -> actix_web::http::StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }
}
#[derive(Debug, Error)]
pub enum TagParseError {
    #[error("Groon tag was empty")]
    EmptyTag,
    #[error("No attribute value")]
    MissingValue{
        attr: String,
    },
    #[error("Unquoted attribute value parameter")]
    UnquotedValue {
        attr: String,
    },
    #[error("Unrecognized tag")]
    Unrecognized(String),
}
