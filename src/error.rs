use thiserror::Error;

#[derive(Error, Debug)]
pub enum BBScriptError {
    #[error("Failed to open game DB file `{0}` with error `{1}`")]
    GameDBOpenError(String, String),
    #[error("Could not decode game DB file `{0}`")]
    GameDBInvalid(String),
    #[error("Input `{0}` does not exist or is a directory")]
    BadInputFile(String),
    #[error("Output file `{0}` already exists, specify overwrite with -o flag")]
    OutputAlreadyExists(String),
    #[error("Unknown instruction with ID/name `{0}`")]
    UnknownFunction(String),
    #[error("No value associated with argument position `{0}` name `{1}`")]
    NoAssociatedValue(String, String),
    #[error(
        "Jump table size of `{0}` is too big! Is the program reading from the correct offset?"
    )]
    IncorrectJumpTableSize(String),
}
