use crate::signalling;

#[derive(Debug)]
pub enum Error {
    Signalling(signalling::Error),
}

impl<E> From<E> for Error
where
    E: Into<signalling::Error>,
{
    fn from(e: E) -> Self {
        Error::Signalling(e.into())
    }
}
