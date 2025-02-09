use thiserror::Error;

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Unable to read file {0}: {1}")]
    UnableToRead(ParsedReplay, ParsedErrorMessage),
    #[error("Unable to parse {0}: {2}")]
    UnableToParse(ParsedReplay, ParsedDocument, ParsedErrorMessage),
    #[error("No battle record detected in {0}")]
    NoBattleRecord(ParsedReplay),
}

#[derive(Debug)]
pub struct ParsedReplay(pub PathBuf);

impl fmt::Display for ParsedReplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<&Path> for ParsedReplay {
    fn from(path: &Path) -> Self {
        ParsedReplay(path.to_path_buf())
    }
}

#[derive(Debug)]
pub struct ParsedDocument(pub String);

impl fmt::Display for ParsedDocument {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ParsedDocument {
    fn from(doc: String) -> Self {
        ParsedDocument(doc)
    }
}

#[derive(Debug)]
pub struct ParsedErrorMessage(pub String);

impl fmt::Display for ParsedErrorMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ParsedErrorMessage {
    fn from(message: String) -> Self {
        ParsedErrorMessage(message)
    }
}
