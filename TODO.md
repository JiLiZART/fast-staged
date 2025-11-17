# TODO

Детальный план реализации функций из ROADMAP.md

## [ ] Backup files in stash before running commands, revert to previous state if command fails

**План реализации:**

1. Добавить функцию `backup_files_to_stash()` которая:
   - Использует `gix` для создания stash с измененными файлами
   - Сохраняет список зарезервированных файлов в структуре состояния
2. Модифицировать `execute_commands()`:
   - Перед запуском команд вызвать backup для всех файлов, которые будут обработаны
   - Сохранить stash reference в `TaskState` или отдельной структуре
3. Добавить функцию `revert_files_from_stash()`:
   - При ошибке команды использовать `gix` для восстановления файлов из stash
   - Вызывать после обнаружения `CommandStatus::Failed`
4. Обновить логику обработки ошибок в `execute_commands()`:
   - После завершения всех команд проверить наличие ошибок
   - Если есть ошибки - вызвать revert для всех файлов из failed команд
5. Добавить опцию в конфиг для включения/выключения этой функции (опционально)

**Изменения в коде:**

- Добавить поле `stash_id: Option<String>` в `TaskState` или создать отдельную структуру `BackupState`
- Добавить методы работы со stash через `gix::Repository::stash()`
- Модифицировать `execute_commands()` для вызова backup/revert

---

## [ ] Add ability to define command timeout

**План реализации:**

1. Расширить структуру `Config`:
   - Добавить поле `timeout: Option<Duration>` на уровне команд или глобально
2. Модифицировать `execute_commands()`:
   - Использовать `tokio::time::timeout()` для обертки выполнения команды
   - При превышении timeout устанавливать статус `CommandStatus::Timeout` (нужно добавить в enum)
3. Обновить `CommandStatus`:
   - Добавить вариант `Timeout` с соответствующим отображением в UI
4. Обновить `StatusDisplay` trait:
   - Добавить обработку `Timeout` статуса (например, оранжевый цвет и символ ⏱)

**Изменения в коде:**

- Добавить `timeout: Option<Duration>` в `Config`
- Добавить `CommandStatus::Timeout`
- Обернуть `Command::output().await` в `tokio::time::timeout()`
- Обновить UI для отображения timeout статуса

---

## [ ] Groups in config, each group has list of patterns to watch and own settings for group like execution order

**План реализации:**

1. Создать новую структуру `Group`:
   ```rust
   struct Group {
       name: String,
       patterns: HashMap<FilePattern, CommandList>,
       execution_order: ExecutionOrder,
       timeout: Option<Duration>,
       // другие настройки
   }
   ```
2. Переработать `Config`:
   - Заменить `patterns: HashMap<FilePattern, CommandList>` на `groups: Vec<Group>`
   - И добавить `global` как верхнеуровневая группа `Group`
3. Обновить `match_files_to_commands()`:
   - Итерироваться по группам вместо прямых паттернов
   - Для каждого файла проверять соответствие паттернам в группах
   - Возвращать структуру с информацией о группе для каждой команды
4. Обновить `TaskState`:
   - Добавить поле `group_name: Option<String>` для отслеживания принадлежности к группе

**Изменения в коде:**

- Создать структуру `Group` с необходимыми полями
- Переработать десериализацию `Config`
- Реализовать поддержку конфига вида

```toml
timeout = "1sec"

[group1.patterns]
"*.(js|ts|tsx|jsx)" = [
    "npm run fmt"
]

[group2]
timeout = "5sec"

[group2.patterns]
"*.css" = [
    "npm run prettier:staged",
    "npm run stylelint:staged"
]

[group3.patterns]
"**/*.rs" = [
    "cargo fmt",
    "cargo clippy",
]
```

- Обновить логику сопоставления файлов с командами
- Обновить UI для отображения информации о группах (опционально)

---

## [ ] Add ability to define execution order, parallel or sequential in each group

**План реализации:**

1. Создать enum `ExecutionOrder`:
   ```rust
   enum ExecutionOrder {
       Parallel,
       Sequential,
   }
   ```
2. Добавить поле `execution_order: ExecutionOrder` в структуру `Group`
3. Модифицировать `execute_commands()`:
   - Группировать команды по группам
   - Для `Parallel` - запускать все команды одновременно (текущее поведение)
   - Для `Sequential` - запускать команды последовательно, ожидая завершения предыдущей
4. Обновить UI:
   - Показывать прогресс выполнения с учетом порядка выполнения
   - Для sequential показывать, какая команда выполняется в данный момент

**Изменения в коде:**

- Добавить enum `ExecutionOrder`
- Добавить поле в `Group`
- Переработать `execute_commands()` для поддержки обоих режимов
- Обновить логику отображения в UI

---

## [ ] Add ability to define timeout of command in each group

**План реализации:**

1. Добавить поле `timeout: Option<Duration>` в структуру `Group`
2. При выполнении команд из группы использовать timeout из группы, если он задан
3. Приоритет: timeout группы > глобальный timeout > без timeout
4. Обновить логику в `execute_commands()`:
   - При создании задачи использовать timeout из соответствующей группы

**Изменения в коде:**

- Добавить `timeout` в `Group`
- Модифицировать создание задач в `execute_commands()` для использования timeout группы
- Обновить обработку timeout ошибок

---

## [ ] Add ability to define command execution behavior, run on each file or pass a list of files to command in each group

**План реализации:**

1. Создать enum `ExecutionBehavior`:
   ```rust
   enum ExecutionBehavior {
       PerFile,      // Запускать команду для каждого файла отдельно
       Batch,        // Передать список файлов одной команде
   }
   ```
2. Добавить поле `execution_behavior: ExecutionBehavior` в `Group`
3. Модифицировать `execute_commands()`:
   - Для `PerFile`: текущее поведение - создавать отдельную задачу для каждого файла
   - Для `Batch`: собирать все файлы группы и передавать их одной команде
   - Для batch режима нужно передавать файлы как аргументы команды или через переменную окружения
4. Обновить `TaskState`:
   - Для batch режима `filename` может быть списком файлов или специальным значением

**Изменения в коде:**

- Создать enum `ExecutionBehavior`
- Добавить поле в `Group`
- Переработать логику создания задач в `execute_commands()`
- Обновить формат передачи файлов в команды (аргументы или env переменные)

---

## [ ] Add ability to define continue execution of commands if any command fails

**План реализации:**

1. Добавить поле `continue_on_error: bool` в структуру `Group` (или глобально в `Config`)
2. Модифицировать логику выполнения:
   - Для sequential режима: при ошибке проверять `continue_on_error`
   - Если `true` - продолжать выполнение следующих команд
   - Если `false` - останавливать выполнение группы
3. Для parallel режима:
   - Все команды выполняются независимо (текущее поведение)
   - Но можно добавить опцию остановки всех команд при первой ошибке
4. Обновить UI:
   - Показывать, что выполнение продолжается несмотря на ошибки
   - Выделять failed команды, но не блокировать остальные

**Изменения в коде:**

- Добавить `continue_on_error: bool` в `Group` или `Config`
- Модифицировать sequential выполнение для проверки этого флага
- Обновить логику обработки ошибок

---

## [ ] Display total affected files count

**План реализации:**

1. В функции `match_files_to_commands()` подсчитывать общее количество уникальных файлов
2. Передавать это значение в `run_ui()` или хранить в структуре состояния
3. Обновить UI в `run_ui()`:
   - Добавить отображение счетчика файлов в заголовке или отдельном блоке
   - Формат: "Affected files: N" или "Processing N files"

**Изменения в коде:**

- Подсчитывать количество файлов в `match_files_to_commands()`
- Передавать счетчик в UI
- Добавить отображение в заголовке или footer

---

## [ ] Display total time of execution

**План реализации:**

1. В `run_ui()` сохранять время начала выполнения всех команд
2. При завершении всех команд вычислять общее время: `Instant::now() - start_time`
3. Отображать в UI:
   - В footer или отдельном блоке после списка команд
   - Формат: "Total execution time: XXXms" или "Total time: X.XXs"
4. Обновлять отображение в реальном времени, показывая текущее время выполнения

**Изменения в коде:**

- Добавить `start_time: Instant` в `run_ui()`
- Вычислять и отображать общее время в footer или отдельном блоке
- Форматировать время в читаемый вид (ms или s)

---

## [ ] Display total time of execution per command

**План реализации:**

1. Уже реализовано частично - каждая команда отслеживает свое время выполнения
2. Улучшить отображение:
   - Показывать время для каждой команды в списке (уже есть для Done/Failed)
   - Добавить группировку по командам и показывать суммарное время для каждой уникальной команды
3. Создать отдельный блок или секцию в UI:
   - Группировать команды по имени
   - Показывать количество выполнений и общее время
   - Формат: "command_name: N executions, total XXXms, avg XXms"

**Изменения в коде:**

- Группировать `TaskState` по командам
- Вычислять суммарное и среднее время для каждой команды
- Добавить отображение статистики в UI

---

## [ ] Display error stderr and stdout in block with border

**План реализации:**

1. Расширить `TaskState`:
   - Добавить поля `stdout: Arc<Mutex<Option<String>>>` и `stderr: Arc<Mutex<Option<String>>>`
2. В `execute_commands()` сохранять вывод команд:
   - При завершении команды сохранять `output.stdout` и `output.stderr` в `TaskState`
3. Обновить UI в `run_ui()`:
   - При выборе failed команды (или автоматически для всех failed) показывать блок с ошибками
   - Использовать `Paragraph` или `Block` с границами для отображения
   - Разделить stdout и stderr на отдельные секции
   - Добавить возможность прокрутки для длинных выводов (опционально)
4. Форматирование:
   - Показывать stderr красным цветом
   - Показывать stdout обычным цветом
   - Ограничить длину вывода или добавить прокрутку

**Изменения в коде:**

- Добавить поля для stdout/stderr в `TaskState`
- Сохранять вывод в `execute_commands()`
- Создать функцию отображения ошибок в UI
- Добавить блок с границами для вывода ошибок

---

## [ ] Add ability to define relative or absolute file paths will be passed to command for each group

**План реализации:**

1. Создать enum `PathFormat`:
   ```rust
   enum PathFormat {
       Relative,  // Относительные пути от корня репозитория
       Absolute,  // Абсолютные пути
   }
   ```
2. Добавить поле `path_format: PathFormat` в структуру `Group`
3. Модифицировать передачу файлов в команды:
   - Для `Relative`: использовать пути как есть (относительно репозитория)
   - Для `Absolute`: преобразовывать относительные пути в абсолютные через `std::fs::canonicalize()` или `Path::absolutize()`
4. Обновить логику в `execute_commands()`:
   - При создании команды преобразовывать пути согласно настройке группы
   - Передавать пути как аргументы или через переменную окружения

**Изменения в коде:**

- Создать enum `PathFormat`
- Добавить поле в `Group`
- Добавить функцию преобразования путей
- Обновить логику передачи файлов в команды

---

## [ ] Add config variations, read .fast-staged.toml, or fast-staged.toml or read fast-staged.json or .fast-staged.json or read "fast-staged" section in package.json

**План реализации:**

1. Создать функцию `find_config_file()` которая:
   - Проверяет наличие файлов в следующем порядке приоритета:
     1. `.fast-staged.toml` (в текущей директории)
     2. `fast-staged.toml` (в текущей директории)
     3. `.fast-staged.json` (в текущей директории)
     4. `fast-staged.json` (в текущей директории)
     5. `package.json` (искать секцию "fast-staged")
   - Возвращает путь к найденному файлу и его тип (toml/json/package.json)
2. Создать функцию `load_config_from_package_json()`:
   - Парсить `package.json` используя `serde_json`
   - Извлекать секцию `"fast-staged"` из объекта
   - Преобразовывать JSON в структуру `Config` (может потребоваться конвертация)
3. Модифицировать `load_config()`:
   - Вызывать `find_config_file()` для поиска конфига
   - В зависимости от типа файла использовать соответствующую функцию загрузки:
     - `.toml` файлы - текущая логика с `toml::from_str()`
     - `.json` файлы - `serde_json::from_str()` или `serde_json::from_value()` если из package.json
     - `package.json` - `load_config_from_package_json()`
4. Обработка ошибок:
   - Если конфиг не найден - возвращать понятную ошибку с указанием проверенных путей
   - Если конфиг найден, но невалидный - возвращать ошибку с указанием файла и проблемы

**Изменения в коде:**

- Создать enum `ConfigSource`:
  ```rust
  enum ConfigSource {
      TomlFile(PathBuf),
      JsonFile(PathBuf),
      PackageJson(PathBuf),
  }
  ```
- Создать функцию `find_config_file() -> Result<ConfigSource>`
- Создать функцию `load_config_from_package_json(path: &Path) -> Result<Config>`
- Переработать `load_config()` для поддержки всех вариантов
- Добавить зависимости `serde_json` в `Cargo.toml` (если еще нет)
- Обновить обработку ошибок для указания источника конфига

**Примеры конфигов:**

```toml
# .fast-staged.toml или fast-staged.toml
timeout = "1sec"
[group1.patterns]
"*.rs" = ["cargo fmt"]
```

```json
// .fast-staged.json или fast-staged.json
{
  "timeout": "1sec",
  "group1": {
    "patterns": {
      "*.rs": ["cargo fmt"]
    }
  }
}
```

```json
// package.json
{
  "name": "my-project",
  "fast-staged": {
    "timeout": "1sec",
    "group1": {
      "patterns": {
        "*.rs": ["cargo fmt"]
      }
    }
  }
}
```

---

## [ ] Add checks and readable errors (create errors enum) for `no config`, `config is not valid`, `no git repository`, `no staged files found` , `no files found matched for group_name patterns`, `failed to execute command, no command found`

**План реализации:**

1. Создать enum `AppError` с вариантами для всех возможных ошибок:

   ```rust
   #[derive(Debug, thiserror::Error)]
   enum AppError {
       #[error("Configuration file not found. Checked: {checked_paths:?}")]
       ConfigNotFound { checked_paths: Vec<PathBuf> },

       #[error("Invalid configuration in {path:?}: {details}")]
       ConfigInvalid { path: PathBuf, details: String },

       #[error("Not a git repository. Current directory: {dir:?}")]
       NotGitRepository { dir: PathBuf },

       #[error("No staged files found")]
       NoStagedFiles,

       #[error("No files matched patterns for group '{group_name}'. Patterns: {patterns:?}")]
       NoFilesMatched { group_name: String, patterns: Vec<String> },

       #[error("Failed to execute command '{command}': {reason}")]
       CommandNotFound { command: String, reason: String },

       #[error("IO error: {0}")]
       IoError(#[from] std::io::Error),

       #[error("Git error: {0}")]
       GitError(#[from] gix::Error),

       // другие ошибки по необходимости
   }
   ```

2. Добавить проверки в соответствующие функции:
   - `load_config()`: проверка наличия конфига, валидности
   - `get_changed_files()`: проверка git репозитория, наличия staged файлов
   - `match_files_to_commands()`: проверка соответствия файлов паттернам групп
   - `execute_commands()`: проверка наличия команд, валидности путей
3. Обновить все функции для возврата `Result<T, AppError>` вместо `Result<T, Box<dyn Error>>`
4. Добавить контекстные сообщения:
   - При ошибке конфига показывать путь к файлу и конкретную проблему
   - При отсутствии staged файлов предлагать выполнить `git add`
   - При отсутствии совпадений показывать какие паттерны проверялись
   - При ошибке команды показывать полный путь и причину (не найдена, нет прав и т.д.)
5. Обновить `main()`:
   - Использовать `color_eyre` или `anyhow` для красивого вывода ошибок
   - Показывать понятные сообщения пользователю

**Изменения в коде:**

- Добавить зависимость `thiserror` в `Cargo.toml` для создания enum ошибок
- Создать enum `AppError` со всеми вариантами ошибок
- Добавить проверки в каждую функцию:

  ```rust
  // В load_config()
  let config_path = find_config_file()?;
  let content = fs::read_to_string(&config_path)
      .map_err(|e| AppError::ConfigNotFound { ... })?;
  let config: Config = toml::from_str(&content)
      .map_err(|e| AppError::ConfigInvalid { path: config_path, details: e.to_string() })?;

  // В get_changed_files()
  let repo = gix::open(".")
      .map_err(|_| AppError::NotGitRepository { dir: current_dir()? })?;
  let changed_files = ...;
  if changed_files.is_empty() {
      return Err(AppError::NoStagedFiles);
  }

  // В match_files_to_commands()
  for group in &config.groups {
      let matched = ...;
      if matched.is_empty() {
          return Err(AppError::NoFilesMatched {
              group_name: group.name.clone(),
              patterns: group.patterns.keys().cloned().collect()
          });
      }
  }

  // В execute_commands()
  for command in commands {
      // Проверка существования команды
      if !command_exists(&command) {
          return Err(AppError::CommandNotFound {
              command: command.clone(),
              reason: "Command not found in PATH".to_string()
          });
      }
  }
  ```

- Обновить сигнатуры функций для использования `AppError`
- Добавить helper функцию `command_exists()` для проверки наличия команды
- Обновить `main()` для обработки и красивого вывода ошибок

**Улучшения UX:**

- Добавить цветной вывод ошибок (красный для ошибок)
- Предлагать решения для типичных проблем:
  - "No config found" → "Create .fast-staged.toml file"
  - "No staged files" → "Run 'git add' to stage files"
  - "Command not found" → "Install the required tool or check PATH"

---

## Приоритет реализации

Рекомендуемый порядок реализации для минимизации конфликтов:

1. **Обработка ошибок** (пункт 16) - критично для стабильности, нужно сделать в первую очередь
2. **Вариации конфигов** (пункт 15) - улучшает UX, можно делать параллельно с обработкой ошибок
3. **Базовые улучшения UI** (пункты 10, 11, 12) - не требуют изменения архитектуры
4. **Группы в конфиге** (пункт 5) - фундаментальное изменение, нужно сделать перед настройками групп
5. **Настройки групп** (пункты 6, 7, 8, 14) - зависят от групп
6. **Обработка ошибок в группах** (пункты 9, 13) - улучшения существующей логики
7. **Backup и timeout** (пункты 1, 2, 4) - дополнительные функции безопасности
