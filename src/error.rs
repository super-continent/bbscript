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
    #[error("Unknown instruction with name `{0}`")]
    UnknownInstructionName(String),
    #[error("Unknown instruction with ID {0} (hex: {0:#X})")]
    UnknownInstructionID(u32),
    #[error("No variable ID associated with `{0}` in config")]
    NoVariableName(String),
    #[error("No enum associated with index argument {0} in instruction {1}`")]
    NoEnum(usize, u32),
    #[error("Argument tried to access nonexistant enum `{0}`")]
    BadEnumReference(String),
    #[error("No value associated with variant `{0}` in enum `{1}`")]
    NoAssociatedValue(String, String),
    #[error(
        "Jump table size of `{0}` is too big! Is the program reading from the correct offset?"
    )]
    IncorrectJumpTableSize(String),
    #[error("Got instruction `{0}` mismatched to size {1}. size defined in config is {2}")]
    IncorrectFunctionSize(String, usize, usize),
    #[error(transparent)]
    PestConsumeError(#[from]pest_consume::Error<crate::rebuilder::Rule>),
    #[error(transparent)]
    FormatError(#[from]std::fmt::Error)
}
