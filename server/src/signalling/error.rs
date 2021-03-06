use tokio_tungstenite::tungstenite;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    WebSocket(tungstenite::Error),
    JSON(serde_json::error::Error),
    Poison,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::IO(e)
    }
}

impl From<tungstenite::Error> for Error {
    fn from(e: tungstenite::Error) -> Self {
        Error::WebSocket(e)
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Error::Poison
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(e: serde_json::error::Error) -> Self {
        Error::JSON(e)
    }
}
