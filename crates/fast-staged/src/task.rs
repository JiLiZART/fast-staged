#[derive(Clone)]
pub struct TaskState {
  filename: String,
  command: String,
  group_name: Option<String>,
  status: Arc<Mutex<CommandStatus>>,
  started_at: Arc<Mutex<Option<Instant>>>,
  duration_ms: Arc<Mutex<Option<u128>>>,
}
