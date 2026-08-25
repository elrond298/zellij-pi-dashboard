use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_width::UnicodeWidthChar;
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
    instance_offsets: Vec<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Session {
    pid: u32,
    pane_id: Option<String>,
    session_name: Option<String>,
    instance_name: Option<String>,
    cwd: Option<String>,
    workspace: Option<Workspace>,
    mode: Option<String>,
    context_usage: Option<ContextStats>,
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
#[serde(default)]
struct AgentArgs {
    name: Option<String>,
    subagent_type: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Agent {
    name: Option<String>,
    description: Option<String>,
    r#type: Option<String>,
    status: String,
    started_at: Option<u64>,
    completed_at: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ContextStats {
    tokens: Option<u64>,
    context_window: u64,
    percent: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Workspace {
    vcs: String,
    root: String,
    name: Option<String>,
    worktree: bool,
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
    fn next_instance(&mut self) {
        if let Some(offset) = self
            .instance_offsets
            .iter()
            .copied()
            .find(|offset| *offset > self.scroll)
            .or_else(|| self.instance_offsets.first().copied())
        {
            self.scroll = offset;
        }
    }

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
            "PI DASHBOARD · {} sessions · {} busy · Tab instances · j/k scroll · Esc close",
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
            let mode = if session.mode.as_deref() == Some("plan") {
                " [PLAN]"
            } else {
                ""
            };
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
                "{state}{mode}  {name}{pane}  {model}  pid:{}",
                session.pid
            ));
            if let Some(cwd) = &session.cwd {
                lines.push(format!("  Project  {cwd}"));
            }
            if let Some(workspace) = &session.workspace {
                let kind = if workspace.vcs == "git" && workspace.worktree {
                    "git worktree"
                } else {
                    workspace.vcs.as_str()
                };
                let name = workspace
                    .name
                    .as_deref()
                    .map(|name| format!(":{name}"))
                    .unwrap_or_default();
                let root = if session.cwd.as_deref() == Some(workspace.root.as_str()) {
                    String::new()
                } else {
                    format!(" · {}", workspace.root)
                };
                lines.push(format!("  Workspace  {kind}{name}{root}"));
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
                    "  Todo  [{}] {percent:>3}% · {}/{} done · {} active · {} pending",
                    progress(percent, width.saturating_sub(58).clamp(8, 28)),
                    todo.completed,
                    todo.total,
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
                let stats = session
                    .goal_detail
                    .as_ref()
                    .map(|goal| {
                        format!(
                            " · {} · {} · iter {} · {} tok · recorded {}s",
                            elapsed(now.saturating_sub(goal.started_at)),
                            goal.status,
                            goal.iteration,
                            compact_tokens(goal.tokens_used),
                            goal.time_used_seconds
                        )
                    })
                    .unwrap_or_default();
                lines.push(format!("  Goal  {goal}{stats}"));
            }

            let active_agents: Vec<_> = session
                .tools
                .iter()
                .filter(|tool| tool.name == "Agent")
                .collect();
            let agents = display_agents(&session.agents);
            let legacy_agent = session.tools.is_empty() && session.tool.as_deref() == Some("Agent");
            let agent_count = active_agents.len() + agents.len() + usize::from(legacy_agent);
            if agent_count > 0 {
                lines.push(format!("  Agents ({agent_count})"));
            }
            for tool in active_agents {
                let args = tool
                    .args
                    .as_deref()
                    .and_then(|args| serde_json::from_str::<AgentArgs>(args).ok())
                    .unwrap_or_default();
                let age = elapsed(now.saturating_sub(tool.started_at));
                lines.push(format!(
                    "    ● {}",
                    agent_summary(
                        args.name.as_deref(),
                        args.subagent_type.as_deref(),
                        &format!("launching {age}"),
                    )
                ));
                if let Some(description) = args.description {
                    lines.push(format!("      {description}"));
                }
            }
            if legacy_agent {
                lines.push("    ● Agent · running".into());
            }
            for agent in agents {
                let age = agent
                    .started_at
                    .map(|time| elapsed(now.saturating_sub(time)))
                    .unwrap_or_default();
                let status = match agent.status.as_str() {
                    "background" => "running",
                    status => status,
                };
                let state = if age.is_empty() {
                    status.into()
                } else {
                    format!("{status} {age}")
                };
                lines.push(format!(
                    "    {} {}",
                    agent_symbol(status),
                    agent_summary(agent.name.as_deref(), agent.r#type.as_deref(), &state)
                ));
                if let Some(description) = &agent.description {
                    lines.push(format!("      {description}"));
                }
            }

            for tool in session.tools.iter().filter(|tool| tool.name != "Agent") {
                let age = elapsed(now.saturating_sub(tool.started_at));
                lines.push(format!(
                    "  Tool  {} · {age} · {}",
                    tool.name,
                    tool.args.as_deref().unwrap_or("")
                ));
            }
            if session.tools.is_empty() {
                if let Some(tool) = &session.tool {
                    if tool != "Agent" {
                        lines.push(format!("  Tool  {tool}"));
                    }
                }
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
                    "  Usage  {} tok · in {} · out {} · cache {} · {} calls · ${:.4} · elapsed {} · busy {}{}",
                    compact_tokens(tokens.total),
                    compact_tokens(tokens.input),
                    compact_tokens(tokens.output),
                    compact_tokens(tokens.cache_read.saturating_add(tokens.cache_write)),
                    tokens.tool_calls,
                    tokens.cost,
                    elapsed(now.saturating_sub(session.started_at)),
                    elapsed(session.busy_ms + active_ms),
                    thinking
                ));
            }
            if let Some(context) = &session.context_usage {
                let used = context
                    .tokens
                    .map(compact_tokens)
                    .unwrap_or_else(|| "?".into());
                let percent = context
                    .percent
                    .or_else(|| {
                        context
                            .tokens
                            .filter(|_| context.context_window > 0)
                            .map(|tokens| tokens as f64 * 100.0 / context.context_window as f64)
                    })
                    .map(|percent| format!(" · {percent:.0}%"))
                    .unwrap_or_default();
                lines.push(format!(
                    "  Context  {used} / {}{percent}",
                    compact_tokens(context.context_window)
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
            EventType::Mouse,
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
            Event::Mouse(Mouse::ScrollDown(lines)) => {
                self.scroll = self.scroll.saturating_add(lines.max(1));
                true
            }
            Event::Mouse(Mouse::ScrollUp(lines)) => {
                self.scroll = self.scroll.saturating_sub(lines.max(1));
                true
            }
            Event::Key(key) if key.has_no_modifiers() => {
                match key.bare_key {
                    BareKey::Tab => self.next_instance(),
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
        // Zellij's table renderer reserves one column for padding.
        let table_width = cols.saturating_sub(1);
        let mut lines = Vec::new();
        let mut instance_offsets = Vec::new();
        for line in self.lines(table_width, now_ms()) {
            if line.starts_with('─') {
                instance_offsets.push(lines.len());
            }
            lines.extend(styled_wrapped_line(&line, table_width));
        }
        let max_scroll = lines.len().saturating_sub(rows);
        for offset in &mut instance_offsets {
            *offset = (*offset).min(max_scroll);
        }
        instance_offsets.dedup();
        self.instance_offsets = instance_offsets;
        self.scroll = self.scroll.min(max_scroll);
        let visible = lines
            .iter()
            .skip(self.scroll)
            .take(rows)
            .fold(Table::new(), |table, line| {
                table.add_styled_row(vec![line.clone()])
            });
        print_table_with_coordinates(visible, 0, 0, Some(cols), Some(rows));
    }
}

fn agent_summary(name: Option<&str>, agent_type: Option<&str>, state: &str) -> String {
    let mut parts: Vec<_> = [name, agent_type]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        parts.push("Agent");
    }
    parts.push(state);
    parts.join(" · ")
}

fn is_running_agent(agent: &Agent) -> bool {
    matches!(agent.status.as_str(), "running" | "background")
}

fn display_agents(agents: &[Agent]) -> Vec<&Agent> {
    let (mut running, mut ended): (Vec<_>, Vec<_>) =
        agents.iter().partition(|agent| is_running_agent(agent));
    ended.sort_by_key(|agent| {
        std::cmp::Reverse(agent.completed_at.or(agent.started_at).unwrap_or(0))
    });
    running.extend(ended.into_iter().take(3));
    running
}

fn agent_symbol(status: &str) -> &'static str {
    match status {
        "running" => "●",
        "completed" => "✓",
        "failed" | "error" => "✗",
        _ => "○",
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

fn compact_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    if tokens < 999_950 {
        return compact_unit(tokens, 1_000, "k");
    }
    compact_unit(tokens, 1_000_000, "M")
}

fn compact_unit(value: u64, divisor: u64, suffix: &str) -> String {
    let tenths = ((value as u128 * 10 + divisor as u128 / 2) / divisor as u128) as u64;
    if tenths % 10 == 0 {
        format!("{}{suffix}", tenths / 10)
    } else {
        format!("{}.{:01}{suffix}", tenths / 10, tenths % 10)
    }
}

fn styled_line_as(line: &str, source: &str) -> Text {
    let text = Text::new(line);
    let content = source.trim_start();
    if source.starts_with("PI DASHBOARD") {
        text.color_all(0)
    } else if source.starts_with('!') {
        text.error_color_all()
    } else if content.starts_with("● BUSY") {
        text.color_substring(1, "● BUSY")
    } else if content.starts_with("○ IDLE") {
        text.success_color_substring("○ IDLE")
    } else if content.starts_with("Todo") || content.starts_with("Agents") {
        text.color_all(0)
    } else if content.starts_with("Goal") {
        text.color_all(3)
    } else if content.starts_with("Tool") || content.starts_with('▶') {
        text.color_all(2)
    } else if content.starts_with('●') {
        text.color_substring(2, "●")
    } else if content.starts_with('✓') {
        text.success_color_all()
    } else if content.starts_with('✗') {
        text.error_color_all()
    } else if source.starts_with('─')
        || content.starts_with("Usage")
        || content.starts_with("Context")
    {
        text.dim_all()
    } else {
        text
    }
}

fn wrap_line(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![String::new()];
    }
    let mut wrapped = Vec::new();
    let mut line = String::new();
    let mut line_width = 0;
    for character in text.chars() {
        let character_width = character.width().unwrap_or(0);
        if line_width + character_width > width && !line.is_empty() {
            wrapped.push(line);
            line = String::new();
            line_width = 0;
        }
        line.push(character);
        line_width += character_width;
    }
    wrapped.push(line);
    wrapped
}

fn styled_wrapped_line(text: &str, width: usize) -> Vec<Text> {
    wrap_line(text, width)
        .into_iter()
        .map(|line| styled_line_as(&line, text))
        .collect()
}

register_plugin!(Dashboard);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_session_details() {
        let json = r#"{"pid":42,"busy":true,"model":"gpt","goal":"ship","progress":50,"todo":{"total":2,"pending":1,"active":0,"completed":1,"items":[{"id":"1","status":"completed","text":"inspect"},{"id":"2","status":"pending","text":"build"}]},"tools":[{"name":"cargo","args":"{\"test\":true}","startedAt":900},{"name":"Agent","args":"{\"name\":\"audit\",\"subagent_type\":\"code-reviewer\",\"description\":\"Review rendering\"}","startedAt":900}],"agents":[{"name":"reviewer","type":"Explore","description":"Locate APIs","status":"background","startedAt":800}],"tokens":{"input":1000,"output":234567,"cacheRead":999000,"total":1234567,"cost":0.01,"toolCalls":2},"startedAt":0,"updatedAt":1000}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        let dashboard = Dashboard {
            sessions: vec![session],
            granted: true,
            ..Default::default()
        };
        let rendered = dashboard.lines(100, 1_000).join("\n");
        assert!(rendered.contains("50%"));
        assert!(rendered.contains("Goal  ship"));
        assert!(rendered.contains("Tool  cargo"));
        assert!(rendered.contains("Agents (2)"));
        assert!(rendered.contains("audit · code-reviewer · launching 0s"));
        assert!(rendered.contains("Review rendering"));
        assert!(rendered.contains("reviewer · Explore · running 0s"));
        assert!(!rendered.contains("subagent_type"));
        assert!(rendered.contains("1.2M tok · in 1k · out 234.6k · cache 999k"));
        assert!(rendered.lines().all(|line| !line.starts_with('>')));
        assert_eq!(compact_tokens(999), "999");
        assert_eq!(compact_tokens(12_400), "12.4k");
        assert_eq!(compact_tokens(2_300_000), "2.3M");
        let wrapped: Vec<_> = ["123456", "ab"]
            .into_iter()
            .flat_map(|line| wrap_line(line, 4))
            .collect();
        assert_eq!(wrapped, ["1234", "56", "ab"]);
        assert_eq!(wrapped.len().saturating_sub(2), 1);
        assert_eq!(wrap_line("你好x", 4), ["你好", "x"]);
        assert_eq!(wrap_line("x", 0), [""]);
        let mut dashboard = Dashboard::default();
        assert!(dashboard.update(Event::Key(KeyWithModifier::new(BareKey::Char('j')))));
        assert_eq!(dashboard.scroll, 1);
        assert!(dashboard.update(Event::Mouse(Mouse::ScrollDown(3))));
        assert_eq!(dashboard.scroll, 4);
        assert!(dashboard.update(Event::Mouse(Mouse::ScrollUp(2))));
        assert_eq!(dashboard.scroll, 2);
        dashboard.scroll = 0;
        dashboard.instance_offsets = vec![2, 10, 15];
        for expected in [2, 10, 15, 2] {
            assert!(dashboard.update(Event::Key(KeyWithModifier::new(BareKey::Tab))));
            assert_eq!(dashboard.scroll, expected);
        }
        let styled = styled_wrapped_line("  Goal  abcdef", 8);
        assert_eq!(styled.len(), 2);
        assert_ne!(styled[1].serialize(), Text::new("abcdef").serialize());
    }

    #[test]
    fn renders_context_workspace_and_plan_mode() {
        let sessions: Vec<Session> = serde_json::from_str(
            r#"[{"pid":1,"busy":true,"cwd":"/tmp/project","mode":"plan","workspace":{"vcs":"jj","root":"/tmp/project","name":"feature"},"contextUsage":{"tokens":85300,"contextWindow":200000,"percent":42.65}},{"pid":2,"cwd":"/tmp/worktree/src","workspace":{"vcs":"git","root":"/tmp/worktree","name":"topic","worktree":true}}]"#,
        )
        .unwrap();
        let dashboard = Dashboard {
            sessions,
            granted: true,
            ..Default::default()
        };
        let rendered = dashboard.lines(120, 1_000).join("\n");
        assert!(rendered.contains("● BUSY [PLAN]"));
        assert!(rendered.contains("Project  /tmp/project"));
        assert!(rendered.contains("Workspace  jj:feature"));
        assert!(rendered.contains("Workspace  git worktree:topic · /tmp/worktree"));
        assert!(rendered.contains("Context  85.3k / 200k · 43%"));
    }

    #[test]
    fn keeps_running_and_three_latest_ended_agents() {
        let agents = [
            ("oldest", "completed", Some(1)),
            ("active", "running", None),
            ("newest", "completed", Some(4)),
            ("middle", "failed", Some(3)),
            ("older", "completed", Some(2)),
        ]
        .map(|(name, status, completed_at)| Agent {
            name: Some(name.into()),
            status: status.into(),
            completed_at,
            ..Default::default()
        });

        let names: Vec<_> = display_agents(&agents)
            .into_iter()
            .filter_map(|agent| agent.name.as_deref())
            .collect();
        assert_eq!(names, ["active", "newest", "middle", "older"]);
    }
}
