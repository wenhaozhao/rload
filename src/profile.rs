use std::fs;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: String,
    pub target: Target,
    #[serde(default)]
    pub runner: Runner,
    #[serde(default)]
    pub load_profile: LoadProfile,
    #[serde(default)]
    pub observability: Observability,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub expression: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runner {
    #[serde(default = "default_threads")]
    pub threads: usize,
    #[serde(default = "default_connections")]
    pub connections: usize,
    pub duration: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout: String,
}

impl Default for Runner {
    fn default() -> Self {
        Self {
            threads: default_threads(),
            connections: default_connections(),
            duration: None,
            timeout: default_timeout(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadProfile {
    pub mode: Option<String>,
    #[serde(rename = "static")]
    pub static_request: Option<StaticRequest>,
    #[serde(rename = "log_replay")]
    pub log_replay: Option<LogReplay>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticRequest {
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogReplay {
    pub path: String,
    pub format: String,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub rounds: Option<u64>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub skip_invalid_records: bool,
    #[serde(default)]
    pub pacing: Option<Pacing>,
    #[serde(default)]
    pub filter: ReplayFilter,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pacing {
    pub mode: String,
    #[serde(default)]
    pub rate: Option<u64>,
    #[serde(default)]
    pub speed: Option<f64>,
    #[serde(default)]
    pub stages: Vec<PacingStage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PacingStage {
    pub duration: String,
    pub target_rps: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayFilter {
    #[serde(default)]
    pub allowed_methods: Vec<String>,
    #[serde(default)]
    pub allowed_uris: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observability {
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub output_html: Option<String>,
}

fn default_threads() -> usize {
    2
}
fn default_connections() -> usize {
    10
}
fn default_timeout() -> String {
    "2s".into()
}
fn default_method() -> String {
    "GET".into()
}

pub fn load(path: impl AsRef<Path>) -> Result<Profile, String> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .map_err(|e| format!("cannot read profile {}: {e}", path.display()))?;
    let profile: Profile = serde_yaml::from_str(&text)
        .map_err(|e| format!("invalid profile {}: {e}", path.display()))?;
    if profile.version != "v1" {
        return Err(format!(
            "profile version must be v1, got {}",
            profile.version
        ));
    }
    if profile.target.url.trim().is_empty() {
        return Err("profile target.url must not be empty".into());
    }
    if let Some(mode) = &profile.load_profile.mode
        && mode != "static"
        && mode != "log_replay"
    {
        return Err(format!(
            "profile load_profile.mode is not supported yet: {mode}"
        ));
    }
    if profile.load_profile.mode.as_deref() == Some("static")
        && profile.load_profile.static_request.is_none()
    {
        return Err("profile load_profile.static is required when mode is static".into());
    }
    if profile.load_profile.mode.as_deref() == Some("log_replay")
        && profile.load_profile.log_replay.is_none()
    {
        return Err("profile load_profile.log_replay is required when mode is log_replay".into());
    }
    if let Some(replay) = &profile.load_profile.log_replay {
        if replay.path.trim().is_empty() {
            return Err("profile load_profile.log_replay.path must not be empty".into());
        }
        if replay.format != "nginx" && replay.format != "jsonl" {
            return Err(format!(
                "profile load_profile.log_replay.format must be nginx or jsonl, got {}",
                replay.format
            ));
        }
        if let Some(rounds) = replay.rounds
            && rounds == 0
        {
            return Err("profile load_profile.log_replay.rounds must be greater than zero".into());
        }
        if let Some(pacing) = &replay.pacing {
            match pacing.mode.as_str() {
                "none" => {}
                "rate" if pacing.rate.is_some_and(|rate| rate > 0) => {}
                "timestamp" if pacing.speed.is_none_or(|speed| speed.is_finite() && speed > 0.0) => {}
                "stages" if !pacing.stages.is_empty() && pacing.stages.iter().all(|stage| stage.target_rps > 0 && !stage.duration.trim().is_empty()) => {}
                "rate" => return Err("profile load_profile.log_replay.pacing.rate must be greater than zero".into()),
                "timestamp" => return Err("profile load_profile.log_replay.pacing.speed must be a finite number greater than zero".into()),
                "stages" => return Err("profile load_profile.log_replay.pacing.stages must contain non-empty durations and positive target_rps values".into()),
                _ => return Err(format!("profile load_profile.log_replay.pacing.mode must be none, rate, timestamp, or stages, got {}", pacing.mode)),
            }
        }
    }
    Ok(profile)
}
