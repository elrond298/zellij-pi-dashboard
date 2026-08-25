use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use zellij_tile::prelude::*;

const READ_STATUSES: &str = r#"
root="${XDG_RUNTIME_DIR:-/tmp}/pi-zellij-status-$(id -u)"
session=$(printf '%s' "$PI_ZELLIJ_SESSION_NAME" | sed 's/[^A-Za-z0-9_.-]/_/g')
dir="$root/session-$session"
for file in "$dir"/*.json; do
  [ -f "$file" ] || continue
  pid=${file##*/}; pid=${pid%.json}
  if kill -0 "$pid" 2>/dev/null; then cat "$file"; printf '\n'; else rm -f "$file"; fi
done
"#;

#[derive(Default)]
struct Dashboard {
    sessions: Vec<Session>,
    error: Option<String>,
    granted: bool,
    session_name: Option<String>,
    loading: bool,
    scroll: usize,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Session {
    pid: u32,
    pane_id: Option<String>,
    session_name: Option<String>,
    instance_name: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    thinking: Option<String>,
    busy: bool,
    tool: Option<String>,
    tools: Vec<Tool>,
    goal: Option<String>,
    goal_detail: Option<Goal>,
    todo: Option<TodoSummary>,
    progress: Option<u8>,
    agents: Vec<Agent>,
    tokens: Option<Tokens>,
    started_at: u64,
    busy_ms: u64,
    activity_started_at: Option<u64>,
    updated_at: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TodoSummary {
    total: usize,
    pending: usize,
    active: usize,
    completed: usize,
    items: Vec<Todo>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Todo {
    id: Option<String>,
    status: String,
    text: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Goal {
    status: String,
    started_at: u64,
    iteration: u64,
    tokens_used: u64,
    time_used_seconds: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Tool {
    name: String,
    args: Option<String>,
    started_at: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Agent {
    name: Option<String>,
    description: Option<String>,
    r#type: Option<String>,
    status: String,
    started_at: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Tokens {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    total: u64,
    cost: f64,
    tool_calls: u64,
}

impl Dashboard {
    fn refresh(&mut self) {
        if self.granted && !self.loading && self.session_name.is_some() {
            self.loading = true;
            let mut environment = BTreeMap::new();
            environment.insert(
                "PI_ZELLIJ_SESSION_NAME".into(),
                self.session_name.clone().unwrap(),
            );
            run_command_with_env_variables_and_cwd(
                &["sh", "-c", READ_STATUSES],
                environment,
                PathBuf::from("."),
                BTreeMap::new(),
            );
        }
    }

    fn accept(&mut self, stdout: Vec<u8>, stderr: Vec<u8>, exit_code: Option<i32>) {
        self.loading = false;
        if exit_code != Some(0) {
            self.error = Some(String::from_utf8_lossy(&stderr).trim().to_owned());
            return;
        }
        let text = String::from_utf8_lossy(&stdout);
        let mut sessions: Vec<_> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        sessions.sort_by_key(|session: &Session| {
            (!session.busy, std::cmp::Reverse(session.updated_at))
        });
        self.sessions = sessions;
        self.error = None;
    }

    fn lines(&self, width: usize, now: u64) -> Vec<String> {
        let busy = self.sessions.iter().filter(|session| session.busy).count();
        let mut lines = vec![format!(
            "PI DASHBOARD  {} sessions  {} busy  [j/k or arrows: scroll]",
            self.sessions.len(),
            busy
        )];
        if let Some(error) = &self.error {
            lines.push(format!("! {error}"));
        }
        if self.sessions.is_empty() {
            lines.push(if self.granted {
                "No Pi sessions found in this Zellij session.".into()
            } else {
                "Grant requested permissions to read Pi session status.".into()
            });
            return lines;
        }

        for session in &self.sessions {
            lines.push("─".repeat(width.min(100)));
            let state = if session.busy { "● BUSY" } else { "○ IDLE" };
            let name = session
                .session_name
                .as_deref()
                .or(session.instance_name.as_deref())
                .unwrap_or("pi");
            let model = session.model.as_deref().unwrap_or("unknown model");
            let pane = session
                .pane_id
                .as_deref()
                .map(|id| format!(" pane:{id}"))
                .unwrap_or_default();
            lines.push(format!(
                "{state}  {name}{pane}  {model}  pid:{}",
                session.pid
            ));
            if let Some(cwd) = &session.cwd {
                lines.push(format!("  cwd  {cwd}"));
            }

            if let Some(todo) = &session.todo {
                let percent = session.progress.unwrap_or_else(|| {
                    if todo.total == 0 {
                        0
                    } else {
                        (todo.completed * 100 / todo.total) as u8
                    }
                });
                lines.push(format!(
                    "  [{}] {percent:>3}%  todos: {} done, {} active, {} pending",
                    progress(percent, width.saturating_sub(58).clamp(8, 28)),
                    todo.completed,
                    todo.active,
                    todo.pending
                ));
                for item in &todo.items {
                    let mark = match item.status.as_str() {
                        "completed" => "✓",
                        "in_progress" => "▶",
                        _ => "·",
                    };
                    let id = item
                        .id
                        .as_deref()
                        .map(|id| format!("#{id} "))
                        .unwrap_or_default();
                    lines.push(format!("    {mark} {id}{}", item.text));
                }
            }

            if let Some(goal) = &session.goal {
                let detail = session.goal_detail.as_ref();
                let age = detail
                    .map(|g| elapsed(now.saturating_sub(g.started_at)))
                    .unwrap_or_default();
                let stats = detail
                    .map(|g| {
                        format!(
                            " [{} · iter {} · {} tok · recorded {}s]",
                            g.status, g.iteration, g.tokens_used, g.time_used_seconds
                        )
                    })
                    .unwrap_or_default();
                lines.push(format!("  goal {goal}  {age}{stats}"));
            }

            for tool in &session.tools {
                let age = elapsed(now.saturating_sub(tool.started_at));
                lines.push(format!(
                    "  tool {}  {age}  {}",
                    tool.name,
                    tool.args.as_deref().unwrap_or("")
                ));
            }
            if session.tools.is_empty() {
                if let Some(tool) = &session.tool {
                    lines.push(format!("  tool {tool}"));
                }
            }

            for agent in &session.agents {
                let label = agent
                    .name
                    .as_deref()
                    .or(agent.description.as_deref())
                    .or(agent.r#type.as_deref())
                    .unwrap_or("agent");
                let age = agent
                    .started_at
                    .map(|time| elapsed(now.saturating_sub(time)))
                    .unwrap_or_default();
                lines.push(format!("  agent {}  {}  {age}", agent.status, label));
            }

            if let Some(tokens) = &session.tokens {
                let active_ms = if session.busy {
                    session
                        .activity_started_at
                        .map(|at| now.saturating_sub(at))
                        .unwrap_or(0)
                } else {
                    0
                };
                let thinking = session
                    .thinking
                    .as_deref()
                    .map(|level| format!(" · think {level}"))
                    .unwrap_or_default();
                lines.push(format!(
                    "  usage {} tok (in {} · out {} · cache r{} w{}) · {} calls · ${:.4} · elapsed {} · busy {}{}",
                    tokens.total, tokens.input, tokens.output, tokens.cache_read, tokens.cache_write,
                    tokens.tool_calls, tokens.cost, elapsed(now.saturating_sub(session.started_at)),
                    elapsed(session.busy_ms + active_ms), thinking
                ));
            }
        }
        lines
    }
}

impl ZellijPlugin for Dashboard {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Timer,
            EventType::RunCommandResult,
            EventType::ModeUpdate,
            EventType::Key,
        ]);
        self.granted = true;
        request_permission(&[
            PermissionType::RunCommands,
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
        ]);
        self.refresh();
        set_timeout(1.0);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::ModeUpdate(mode) => {
                self.session_name = mode.session_name;
                self.refresh();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Granted) => {
                self.granted = true;
                self.refresh();
                true
            }
            Event::PermissionRequestResult(PermissionStatus::Denied) => {
                self.granted = false;
                self.error = Some("Required permissions denied".into());
                true
            }
            Event::Timer(_) => {
                self.refresh();
                set_timeout(1.0);
                true
            }
            Event::RunCommandResult(code, stdout, stderr, _) => {
                self.accept(stdout, stderr, code);
                true
            }
            Event::Key(key) if key.has_no_modifiers() => {
                match key.bare_key {
                    BareKey::Esc => close_focus(),
                    BareKey::Down | BareKey::Char('j') => self.scroll += 1,
                    BareKey::Up | BareKey::Char('k') => self.scroll = self.scroll.saturating_sub(1),
                    BareKey::Home | BareKey::Char('g') => self.scroll = 0,
                    _ => return false,
                }
                true
            }
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let lines = self.lines(cols, now_ms());
        let max_scroll = lines.len().saturating_sub(rows);
        self.scroll = self.scroll.min(max_scroll);
        let visible = lines
            .iter()
            .skip(self.scroll)
            .take(rows)
            .map(|line| styled_line(&clip(line, cols)))
            .collect();
        print_nested_list_with_coordinates(visible, 0, 0, Some(cols), Some(rows));
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn progress(percent: u8, width: usize) -> String {
    let filled = width * percent.min(100) as usize / 100;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn elapsed(ms: u64) -> String {
    let seconds = ms / 1000;
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, seconds / 60 % 60),
    }
}

fn styled_line(line: &str) -> NestedListItem {
    let text = NestedListItem::new(line);
    let content = line.trim_start();
    if line.starts_with("PI DASHBOARD") {
        text.color_all(0)
    } else if line.starts_with('!') {
        text.error_color_all()
    } else if content.starts_with("● BUSY") {
        text.color_substring(1, "● BUSY")
    } else if content.starts_with("○ IDLE") {
        text.success_color_substring("○ IDLE")
    } else if content.starts_with('[') || content.starts_with("agent ") {
        text.color_all(0)
    } else if content.starts_with("goal ") {
        text.color_all(3)
    } else if content.starts_with("tool ") || content.starts_with('▶') {
        text.color_all(2)
    } else if content.starts_with('✓') {
        text.success_color_all()
    } else {
        text
    }
}

fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.into()
    } else {
        text.chars()
            .take(width.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

register_plugin!(Dashboard);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_session_details() {
        let json = r#"{"pid":42,"busy":true,"model":"gpt","goal":"ship","progress":50,"todo":{"total":2,"pending":1,"active":0,"completed":1,"items":[{"id":"1","status":"completed","text":"inspect"},{"id":"2","status":"pending","text":"build"}]},"tools":[{"name":"cargo","args":"{\"test\":true}","startedAt":900}],"agents":[{"name":"reviewer","status":"background","startedAt":800}],"tokens":{"input":10,"output":5,"total":15,"cost":0.01,"toolCalls":1},"startedAt":0,"updatedAt":1000}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        let dashboard = Dashboard {
            sessions: vec![session],
            granted: true,
            ..Default::default()
        };
        let rendered = dashboard.lines(100, 1_000).join("\n");
        assert!(
            rendered.contains("50%")
                && rendered.contains("goal ship")
                && rendered.contains("tool cargo")
                && rendered.contains("agent background")
                && rendered.contains("15 tok")
        );
        assert!(
            styled_line("  goal ship").serialize()
                != NestedListItem::new("  goal ship").serialize()
        );
    }
}
