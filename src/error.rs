#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Grpc(#[from] tonic::Status),
    #[error(transparent)]
    Transport(#[from] tonic::transport::Error),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    #[error("failed to open file")]
    File(#[from] std::io::Error),
    #[error("failed to draw chart")]
    Draw(#[from] crate::draw::DrawError),
    #[error("failed to parse ACMI (Tacview) file")]
    Tracview(#[from] tacview::ParseError),
    #[error("failed to send Discord message")]
    Discord(#[from] serenity::prelude::SerenityError),
    #[error("failed to deserialize JSON")]
    Serde(#[from] serde_json::Error),
}
