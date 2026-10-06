use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub memory: String,
    pub cpus: f64,
    pub pids: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seccomp {
    pub blocked: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rootfs: String,
    pub command: Vec<String>,
    pub hostname: String,
    pub limits: Limits,
    pub seccomp: Seccomp,
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Config> {
        let text = std::fs::read_to_string(path)?;
        let config = toml::from_str(&text)?;
        Ok(config)
    }
}
