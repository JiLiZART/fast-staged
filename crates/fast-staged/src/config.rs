type FilePattern = String;
type CommandList = Vec<String>;

#[derive(Debug, Clone)]
struct Group {
  name: String,
  patterns: HashMap<FilePattern, CommandList>,
  timeout: Option<String>,
  execution_order: ExecutionOrder,
}

#[derive(Debug, Clone)]
pub enum ConfigSource {
  TomlFile(PathBuf),
  JsonFile(PathBuf),
  PackageJson(PathBuf),
}

#[derive(Debug, Deserialize)]
pub struct Config {
  // Глобальный timeout (опционально)
  #[serde(default)]
  timeout: Option<String>,

  // Группы с паттернами и командами
  // Используем HashMap для динамических ключей групп
  #[serde(flatten)]
  groups: HashMap<String, GroupConfig>,
}

#[derive(Debug, Deserialize)]
pub struct GroupConfig {
  // Timeout для группы (опционально)
  #[serde(default)]
  timeout: Option<String>,

  // Порядок выполнения команд в группе
  // "parallel" (по умолчанию) или "sequential"
  #[serde(default)]
  execution_order: Option<ExecutionOrder>,

  // Паттерны и команды для группы
  patterns: HashMap<FilePattern, CommandList>,
}

pub fn find_config_file() -> Result<ConfigSource> {
  let current_dir = std::env::current_dir()?;
  let mut checked_paths = Vec::new();

  // Порядок проверки файлов
  let candidates: Vec<(&str, fn(PathBuf) -> ConfigSource)> = vec![
    (".fast-staged.toml", ConfigSource::TomlFile),
    ("fast-staged.toml", ConfigSource::TomlFile),
    (".fast-staged.json", ConfigSource::JsonFile),
    ("fast-staged.json", ConfigSource::JsonFile),
    ("package.json", ConfigSource::PackageJson),
  ];

  for (filename, source_fn) in candidates {
    let path = current_dir.join(filename);
    checked_paths.push(path.clone());

    if path.exists() {
      return Ok(source_fn(path));
    }
  }

  Err(AppError::ConfigNotFound { checked_paths })
}

pub fn load_config_from_package_json(path: &Path) -> Result<Config> {
  let content = fs::read_to_string(path).map_err(|e| AppError::ConfigInvalid {
    path: path.to_path_buf(),
    details: format!("Failed to read package.json: {}", e),
  })?;

  let json: Value = serde_json::from_str(&content).map_err(|e| AppError::ConfigInvalid {
    path: path.to_path_buf(),
    details: format!("Invalid JSON in package.json: {}", e),
  })?;

  let fast_staged = json
    .get("fast-staged")
    .ok_or_else(|| AppError::ConfigInvalid {
      path: path.to_path_buf(),
      details: "No 'fast-staged' section found in package.json".to_string(),
    })?;

  // Конвертируем JSON в Config
  // Для простоты используем serde_json для десериализации
  let config: Config =
    serde_json::from_value(fast_staged.clone()).map_err(|e| AppError::ConfigInvalid {
      path: path.to_path_buf(),
      details: format!("Invalid 'fast-staged' section: {}", e),
    })?;

  Ok(config)
}

pub fn load_config() -> Result<Config> {
  let source = find_config_file()?;

  match source {
    ConfigSource::TomlFile(path) => {
      let config_content = fs::read_to_string(&path).map_err(|e| AppError::ConfigInvalid {
        path: path.clone(),
        details: format!("Failed to read file: {}", e),
      })?;

      let config: Config =
        toml::from_str(&config_content).map_err(|e| AppError::ConfigInvalid {
          path: path.clone(),
          details: format!("Invalid TOML: {}", e),
        })?;

      Ok(config)
    }
    ConfigSource::JsonFile(path) => {
      let config_content = fs::read_to_string(&path).map_err(|e| AppError::ConfigInvalid {
        path: path.clone(),
        details: format!("Failed to read file: {}", e),
      })?;

      let config: Config =
        serde_json::from_str(&config_content).map_err(|e| AppError::ConfigInvalid {
          path: path.clone(),
          details: format!("Invalid JSON: {}", e),
        })?;

      Ok(config)
    }
    ConfigSource::PackageJson(path) => load_config_from_package_json(&path),
  }
}

fn parse_groups_from_config(config: &Config) -> Vec<Group> {
  let mut groups = Vec::new();

  for (group_name, group_config) in &config.groups {
    groups.push(Group {
      name: group_name.clone(),
      patterns: group_config.patterns.clone(),
      timeout: group_config.timeout.clone().or(config.timeout.clone()),
      execution_order: group_config
        .execution_order
        .unwrap_or(ExecutionOrder::Parallel),
    });
  }

  groups
}
