use thiserror::Error;

#[derive(Error, Debug)]
pub enum BBScriptError {
    #[error("File `{0}` does not exist")]
    FileDoesNotExist(String),
    #[error("Output file `{0}` already exists, specify overwrite with -o flag")]
    OutputAlreadyExists(String),
}
