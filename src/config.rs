use std::path::PathBuf;
use std::time::Duration;

use crate::RunError;

pub const MAX_REQUEST_BODY_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

impl Method {
    pub const ALL: [Self; 7] = [
        Self::Get,
        Self::Head,
        Self::Post,
        Self::Put,
        Self::Patch,
        Self::Delete,
        Self::Options,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug)]
pub struct RunConfig {
    pub url: String,
    pub method: Method,
    pub limit: RunLimit,
    pub connections: usize,
    pub threads: usize,
    pub timeout: Duration,
}

impl RunConfig {
    pub(crate) fn validate(&self) -> Result<(), RunError> {
        if self.connections == 0 {
            return Err(RunError::InvalidConfig(
                "connections must be greater than zero".into(),
            ));
        }
        if self.threads == 0 {
            return Err(RunError::InvalidConfig(
                "threads must be greater than zero".into(),
            ));
        }
        if self.timeout > Duration::from_secs(60 * 60) {
            return Err(RunError::InvalidConfig(
                "timeout must not exceed one hour".into(),
            ));
        }
        match self.limit {
            RunLimit::Requests(0) => Err(RunError::InvalidConfig(
                "requests must be greater than zero".into(),
            )),
            RunLimit::Duration(duration) if duration.is_zero() => Err(RunError::InvalidConfig(
                "duration must be greater than zero".into(),
            )),
            _ => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunLimit {
    Requests(u64),
    Duration(Duration),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReplayOrder {
    #[default]
    Sequential,
    Shuffle,
    Random,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReplayStage {
    pub duration: Duration,
    pub rate: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayOptions {
    pub order: ReplayOrder,
    pub seed: Option<u64>,
    pub rate: Option<u64>,
    pub timestamps: bool,
    pub speed: f64,
    pub stages: Vec<ReplayStage>,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            order: ReplayOrder::Sequential,
            seed: None,
            rate: None,
            timestamps: false,
            speed: 1.0,
            stages: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReplayRunOptions {
    pub replay: ReplayOptions,
    pub rounds: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RequestFileReplayOptions {
    pub replay: ReplayOptions,
    pub rounds: Option<u64>,
    pub schema: Option<PathBuf>,
    pub skip_invalid_records: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RequestOptions {
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReplayFilter {
    pub allowed_methods: Vec<Method>,
    pub allowed_uris: Vec<String>,
}
