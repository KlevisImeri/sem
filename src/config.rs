use std::fs;
use std::path::Path;

const DEFAULT_MODEL: &str = "nomic-embed-text";
const DEFAULT_MAX_CHUNK_CHARS: usize = 5000;

fn default_max_chunk_chars() -> usize {
    DEFAULT_MAX_CHUNK_CHARS
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub model: String,
    #[serde(default = "default_max_chunk_chars")]
    pub max_chunk_chars: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            max_chunk_chars: DEFAULT_MAX_CHUNK_CHARS,
        }
    }
}

impl Config {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("config.toml");
        if path.exists() {
            let content = fs::read_to_string(&path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        let content = toml::to_string_pretty(self).unwrap();
        fs::write(dir.join("config.toml"), content)
    }
}
