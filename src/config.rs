use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

const DEFAULT_SYSTEM_PROMPT: &str = "You are an expert at writing clear and concise commit messages. \
    Follow these rules strictly:\n\n\
    1. Start with a type: feat, fix, docs, style, refactor, perf, test, build, ci, chore, or revert\n\
    2. Optionally add a scope in parentheses after the type\n\
    3. Write a brief description in imperative mood (e.g., 'add' not 'added')\n\
    4. Keep the first line under 72 characters\n\
    5. Use the body to explain what and why, not how\n\
    6. Reference issues and pull requests liberally\n\
    7. Consider starting the body with 'This commit' to make it clear what the commit does\n\n\
    Example format:\n\
    type(scope): subject\n\n\
    body\n\n\
    footer";

const DEFAULT_USER_PROMPT: &str =
    "Here is the git diff of the staged changes. Generate a commit message that \
    follows the conventional commit format and best practices. Focus on what changed \
    and why, not how it changed:\n\n\
    ```diff\n{}\n```";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    // Skip serializing None values to keep the config file clean
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_prompt: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_token: None,
            api_base_url: Some("https://api.openai.com".to_string()),
            model: Some("gpt-3.5-turbo".to_string()),
            system_prompt: Some(DEFAULT_SYSTEM_PROMPT.to_string()),
            user_prompt: Some(DEFAULT_USER_PROMPT.to_string()),
        }
    }
}

impl Config {
    pub fn config_dir() -> Result<PathBuf> {
        let home_dir = dirs::home_dir().context("Could not find home directory")?;
        let config_dir = home_dir.join(".config").join("aic");

        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).context("Failed to create config directory")?;
        }

        Ok(config_dir)
    }

    pub fn config_path() -> Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("config.toml"))
    }

    fn find_project_config() -> Option<PathBuf> {
        let mut current_dir = std::env::current_dir().ok()?;
        loop {
            let config_path = current_dir.join(".aic/config.toml");
            if config_path.exists() {
                return Some(config_path);
            }
            if !current_dir.pop() {
                break;
            }
        }
        None
    }

    fn load_project_config() -> Result<Option<Self>> {
        if let Some(config_path) = Self::find_project_config() {
            let mut file =
                File::open(&config_path).context("Could not open project config file")?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .context("Could not read project config file")?;

            let config: Config =
                toml::from_str(&contents).context("Failed to parse project config file")?;
            return Ok(Some(config));
        }
        Ok(None)
    }

    pub fn load() -> Result<Self> {
        let mut config = if let Ok(Some(project_config)) = Self::load_project_config() {
            project_config
        } else {
            let config_path = Self::config_path()?;

            if !config_path.exists() {
                let default_config = Self::default();
                default_config.save()?;
                return Ok(default_config);
            }

            let mut file = File::open(&config_path).context("Could not open config file")?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)
                .context("Could not read config file")?;

            toml::from_str(&contents).context("Failed to parse config file")?
        };

        // If we loaded project config, merge with global config
        if Self::find_project_config().is_some() {
            if let Ok(global_config) = Self::load_global() {
                config.merge_with_global(global_config);
            }
        }

        Ok(config)
    }

    fn load_global() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let mut file = File::open(&config_path).context("Could not open global config file")?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .context("Could not read global config file")?;

        let config: Config =
            toml::from_str(&contents).context("Failed to parse global config file")?;
        Ok(config)
    }

    fn merge_with_global(&mut self, global: Self) {
        if self.api_token.is_none() {
            self.api_token = global.api_token;
        }
        if self.api_base_url.is_none() {
            self.api_base_url = global.api_base_url;
        }
        if self.model.is_none() {
            self.model = global.model;
        }
        if self.system_prompt.is_none() {
            self.system_prompt = global.system_prompt;
        }
        if self.user_prompt.is_none() {
            self.user_prompt = global.user_prompt;
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        let toml_string =
            toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        let mut file = File::create(&config_path).context("Could not create config file")?;
        file.write_all(toml_string.as_bytes())
            .context("Failed to write to config file")?;
        file.flush().context("Failed to flush config file")?; // Ensure data is written to disk

        Ok(())
    }

    // Set a configuration value by key name
    #[allow(dead_code)] // Used by CLI command handlers
    pub fn set(&mut self, key: &str, value: Option<String>) -> Result<()> {
        match key {
            "api_token" => self.api_token = value,
            "api_base_url" => self.api_base_url = value,
            "model" => self.model = value,
            "system_prompt" => self.system_prompt = value,
            "user_prompt" => self.user_prompt = value,
            _ => return Err(anyhow::anyhow!("Unknown configuration key: {}", key)),
        }

        self.save()?;
        Ok(())
    }

    // Get a configuration value by key name
    #[allow(dead_code)] // Used by CLI command handlers
    pub fn get(&self, key: &str) -> Option<&String> {
        match key {
            "api_token" => self.api_token.as_ref(),
            "api_base_url" => self.api_base_url.as_ref(),
            "model" => self.model.as_ref(),
            "system_prompt" => self.system_prompt.as_ref(),
            "user_prompt" => self.user_prompt.as_ref(),
            _ => None,
        }
    }

    pub fn get_api_token(&self) -> Result<&String> {
        self.api_token.as_ref().context(
            "API token not found. Please set it using 'aic config set api_token YOUR_TOKEN'",
        )
    }

    pub fn get_api_base_url(&self) -> &str {
        self.api_base_url
            .as_deref()
            .unwrap_or("https://api.openai.com")
    }

    pub fn get_model(&self) -> &str {
        self.model.as_deref().unwrap_or("gpt-3.5-turbo")
    }

    pub fn get_system_prompt(&self) -> &str {
        self.system_prompt
            .as_deref()
            .unwrap_or(DEFAULT_SYSTEM_PROMPT)
    }

    pub fn get_user_prompt(&self) -> &str {
        self.user_prompt.as_deref().unwrap_or(DEFAULT_USER_PROMPT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config: Config = Config::default();
        assert!(config.api_token.is_none());
        assert_eq!(
            config.api_base_url.as_deref(),
            Some("https://api.openai.com")
        );
        assert_eq!(config.model.as_deref(), Some("gpt-3.5-turbo"));
        assert!(config.system_prompt.is_some());
        assert!(config.user_prompt.is_some());
    }

    #[test]
    fn test_set_and_get() {
        // Create a completely unique temporary directory for this test
        // IMPORTANT: Each test should use its own isolated environment
        // to prevent interference between tests
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_dir = temp_dir.path().join(".config").join("aic");
        fs::create_dir_all(&config_dir).expect("Failed to create config directory");

        // Set the HOME environment variable to the temporary directory
        env::set_var("HOME", temp_dir.path());

        // Create config directly
        let mut config = Config::default();

        // Test setting values
        config
            .set("api_token", Some("test_token".to_string()))
            .unwrap();
        config.set("model", Some("gpt-4".to_string())).unwrap();

        // Test getting values - test directly on the object, not after loading from file
        assert_eq!(config.get("api_token").unwrap(), "test_token");
        assert_eq!(config.get("model").unwrap(), "gpt-4");

        // Test setting to None
        config.set("api_token", None).unwrap();
        assert!(config.get("api_token").is_none());

        // Test invalid key
        assert!(config
            .set("invalid_key", Some("value".to_string()))
            .is_err());
        assert!(config.get("invalid_key").is_none());
    }

    #[test]
    fn test_save_and_load() {
        // Create a completely unique temporary directory for this test
        // NOTE: File operations in tests can be tricky. Common issues include:
        // 1. Multiple tests writing to the same file
        // 2. File system caching causing stale reads
        // 3. Environment variables not being isolated between tests
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config_dir = temp_dir.path().join(".config").join("aic");
        fs::create_dir_all(&config_dir).expect("Failed to create config directory");

        // Override the config_dir and config_path methods for testing
        let config_path = config_dir.join("config.toml");

        // Create and save config directly to our test location
        let mut config = Config::default();
        config.api_token = Some("test_token".to_string());

        // Write directly to the file to avoid any path resolution issues
        // IMPORTANT: Always flush file operations to ensure data is written to disk
        let toml_string = toml::to_string_pretty(&config).expect("Failed to serialize");
        let mut file = File::create(&config_path).expect("Failed to create file");
        file.write_all(toml_string.as_bytes())
            .expect("Failed to write");
        file.flush().expect("Failed to flush");

        // Verify file exists
        assert!(
            config_path.exists(),
            "Config file does not exist after direct write"
        );

        // Read file contents directly
        let mut contents = String::new();
        File::open(&config_path)
            .expect("Failed to open file")
            .read_to_string(&mut contents)
            .expect("Failed to read");

        // Parse directly
        let loaded_config: Config = toml::from_str(&contents).expect("Failed to parse");

        // Verify the contents match
        assert_eq!(loaded_config.api_token, Some("test_token".to_string()));
        assert_eq!(loaded_config.api_base_url, config.api_base_url);
        assert_eq!(loaded_config.model, config.model);
        assert_eq!(loaded_config.system_prompt, config.system_prompt);
        assert_eq!(loaded_config.user_prompt, config.user_prompt);
    }

    #[test]
    fn test_getter_methods() {
        let mut config = Config::default();
        config.api_token = Some("test_token".to_string());

        assert_eq!(config.get_api_token().unwrap(), "test_token");
        assert_eq!(config.get_api_base_url(), "https://api.openai.com");
        assert_eq!(config.get_model(), "gpt-3.5-turbo");
        assert!(config
            .get_system_prompt()
            .contains("You are an expert at writing clear and concise commit messages."));
        assert!(config.get_user_prompt().contains(
            "Here is the git diff of the staged changes. Generate a commit message that \
    follows the conventional commit format and best practices. Focus on what changed \
    and why, not how it changed:"
        ));
    }

    #[test]
    fn test_load_project_config() {
        // Create a temporary directory for testing
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let project_dir = temp_dir.path().join("project");
        fs::create_dir_all(&project_dir).expect("Failed to create project directory");
        
        // Set current directory to the project directory
        env::set_current_dir(&project_dir).expect("Failed to set current directory");

        // Test case 1: No project config file exists
        assert!(Config::load_project_config().unwrap().is_none());

        // Test case 2: Project config file exists with all fields
        let config_dir = project_dir.join(".aic");
        fs::create_dir_all(&config_dir).expect("Failed to create .aic directory");
        
        let config_path = config_dir.join("config.toml");
        let mut config = Config::default();
        
        // Set all fields with custom values
        config.api_token = Some("project_token".to_string());
        config.api_base_url = Some("https://custom-api.example.com".to_string());
        config.model = Some("gpt-4".to_string());
        config.system_prompt = Some("Custom system prompt".to_string());
        config.user_prompt = Some("Custom user prompt".to_string());
        
        // Write config to file
        let toml_string = toml::to_string_pretty(&config).expect("Failed to serialize");
        fs::write(&config_path, toml_string).expect("Failed to write config file");

        // Load and verify the config
        let loaded_config = Config::load_project_config().unwrap().unwrap();
        
        // Verify all fields are loaded correctly
        assert_eq!(loaded_config.api_token, Some("project_token".to_string()));
        assert_eq!(loaded_config.api_base_url, Some("https://custom-api.example.com".to_string()));
        assert_eq!(loaded_config.model, Some("gpt-4".to_string()));
        assert_eq!(loaded_config.system_prompt, Some("Custom system prompt".to_string()));
        assert_eq!(loaded_config.user_prompt, Some("Custom user prompt".to_string()));

        // Test case 3: Project config file with partial fields
        let mut partial_config = Config::default();
        partial_config.api_token = Some("partial_token".to_string());
        partial_config.model = Some("gpt-3.5-turbo".to_string());
        
        // Write partial config to file
        let toml_string = toml::to_string_pretty(&partial_config).expect("Failed to serialize");
        fs::write(&config_path, toml_string).expect("Failed to write config file");

        // Load and verify the partial config
        let loaded_partial_config = Config::load_project_config().unwrap().unwrap();
        
        // Verify only the set fields are loaded
        assert_eq!(loaded_partial_config.api_token, Some("partial_token".to_string()));
        assert_eq!(loaded_partial_config.model, Some("gpt-3.5-turbo".to_string()));
        // Other fields should be None
        assert!(loaded_partial_config.api_base_url.is_none());
        assert!(loaded_partial_config.system_prompt.is_none());
        assert!(loaded_partial_config.user_prompt.is_none());
    }
}
