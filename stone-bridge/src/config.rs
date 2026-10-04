use std::collections::HashMap;
use std::path::PathBuf;
use std::{env, fs, panic};

const LOG_LEVEL_KEY: &str = "LOG_LEVEL";
const LOG_PATH_KEY: &str = "LOG_PATH";

pub struct Config {
    pub log_level: String,
    pub log_path: PathBuf,
}
impl Config {
    pub fn load() -> Self {
        let mut config_file_path = Self::get_config_dir();
        config_file_path.push("config");
        if !config_file_path.exists() {
            panic!(
                "Config file does not exist. Please make sure to create a config file under {}",
                config_file_path.to_str().unwrap()
            )
        }
        let config_string =
            fs::read_to_string(&config_file_path).expect("Could not read config file!");
        let config_map = Config::parse_config(config_string);
        let log_level = config_map
            .get(LOG_LEVEL_KEY)
            .map(|s| s.to_owned())
            .unwrap_or("Warn".to_string());
        let log_path = config_map
            .get(LOG_PATH_KEY)
            .map(PathBuf::from)
            .unwrap_or_else(|| Self::get_config_dir().join("log"));
        Self {
            log_level,
            log_path,
        }
    }

    fn get_config_dir() -> PathBuf {
        let config_path = env::var("HOME").unwrap_or("/root".to_string()) + "/.config/stonebridge/";
        PathBuf::from(config_path)
    }

    fn parse_config(config_string: String) -> HashMap<String, String> {
        let mut config = HashMap::new();
        for line in config_string.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                config.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
        config
    }
    pub fn init_sdk_tmp_directory() {
        let sdk_dir_path = PathBuf::from("/tmp/sdk");
        fs::create_dir_all(sdk_dir_path)
            .expect("could not initialize the sdk directory in /tmp/sdk");
    }
}
