use std::borrow::Cow;
use std::fmt;

#[derive(Debug)]
pub struct Error;

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("password preparation failed")
    }
}

impl std::error::Error for Error {}

pub fn saslprep(input: &str) -> Result<Cow<'_, str>, Error> {
    Ok(Cow::Borrowed(input))
}
