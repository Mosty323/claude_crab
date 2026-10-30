use serde_json::{json, Value};
use std::path::PathBuf;

pub const TIMEOUT_TICKS: u64 = 90 * 30;
pub const TEST_TICKS: u64 = 60 * 30;

pub const EVENTS: [&str; 10] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    Edit,
    Bash,
    Read,
    Web,
    Tool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cc {
    Think,
    Work(Act),
    Ask,
    Fail,
    Done,
    Hello,
    Bye,
}

impl Cc {
    fn priority(self) -> u8 {
        match self {
            Cc::Ask => 4,
            Cc::Fail => 3,
            Cc::Work(_) => 2,
            Cc::Think => 1,
            _ => 0,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "think" => Cc::Think,
            "edit" => Cc::Work(Act::Edit),
            "bash" => Cc::Work(Act::Bash),
            "read" => Cc::Work(Act::Read),
            "web" => Cc::Work(Act::Web),
            "tool" => Cc::Work(Act::Tool),
            "ask" => Cc::Ask,
            "fail" => Cc::Fail,
            "done" => Cc::Done,
            "hello" => Cc::Hello,
            "bye" => Cc::Bye,
            _ => return None,
        })
    }

    pub fn label(self, en: bool) -> &'static str {
        match (self, en) {
            (Cc::Work(Act::Edit), true) => "Editing code",
            (Cc::Work(Act::Edit), false) => "Code wird bearbeitet",
            (Cc::Work(Act::Bash), true) => "Running command",
            (Cc::Work(Act::Bash), false) => "Befehl läuft",
            (Cc::Work(Act::Read), true) => "Reading files",
            (Cc::Work(Act::Read), false) => "Liest Dateien",
            (Cc::Work(Act::Web), true) => "Searching the web",
            (Cc::Work(Act::Web), false) => "Sucht im Web",
            (Cc::Work(Act::Tool), true) => "Using tool",
            (Cc::Work(Act::Tool), false) => "Nutzt Werkzeug",
            (Cc::Fail, true) => "Tool failed",
            (Cc::Fail, false) => "Werkzeug fehlgeschlagen",
            (Cc::Done, true) => "Done!",
            (Cc::Done, false) => "Fertig!",
            (Cc::Hello, true) => "Hi!",
            (Cc::Hello, false) => "Hallo!",
            _ => "",
        }
    }
}

pub fn hook_state(v: &Value) -> Option<&'static str> {
    let tool = v["tool_name"].as_str().unwrap_or("");
    Some(match v["hook_event_name"].as_str()? {
        "UserPromptSubmit" | "PostToolUse" => "think",
        "PreToolUse" => match tool {
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => "edit",
            "Bash" | "PowerShell" => "bash",
            "Read" | "Grep" | "Glob" => "read",
            "WebSearch" | "WebFetch" => "web",
            _ => "tool",
        },
        "PermissionRequest" => "ask",
        "Notification" => match v["notification_type"].as_str()? {
            "permission_prompt" | "elicitation_dialog" | "agent_needs_input" => "ask",
            _ => return None,
        },
        "PostToolUseFailure" | "StopFailure" => "fail",
        "Stop" => "done",
        "SessionStart" => "hello",
        "SessionEnd" => "bye",
        _ => return None,
    })
}

pub fn hook_command(v: &Value) -> Option<String> {
    let sid = v["session_id"].as_str().unwrap_or("-");
    let sid: String = sid.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(64).collect();
    hook_state(v).map(|s| format!("cc:{s}:{sid}"))
}

pub fn run_hook() {
    use std::io::Read;
    let mut buf = String::new();
    if std::io::stdin().read_to_string(&mut buf).is_err() {
        return;
    }
    if let Some(cmd) = serde_json::from_str::<Value>(&buf).ok().as_ref().and_then(hook_command) {
        crate::petwin::send(&cmd);
    }
}

struct Session {
    id: String,
    state: Cc,
    at: u64,
}

#[derive(Default)]
pub struct Tracker {
    sessions: Vec<Session>,
    test_until: u64,
}

impl Tracker {
    pub fn event(&mut self, state: Cc, sid: &str, now: u64) -> bool {
        if sid == "test" {
            if now >= self.test_until {
                self.sessions.clear();
            }
            self.test_until = now + TEST_TICKS;
        } else if now < self.test_until {
            return false;
        }
        match state {
            Cc::Done | Cc::Bye => self.sessions.retain(|s| s.id != sid),
            Cc::Fail => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == sid) {
                    s.at = now;
                }
            }
            Cc::Hello => {}
            _ => match self.sessions.iter_mut().find(|s| s.id == sid) {
                Some(s) => {
                    s.state = state;
                    s.at = now;
                }
                None => self.sessions.push(Session { id: sid.to_string(), state, at: now }),
            },
        }
        true
    }

    pub fn expire(&mut self, now: u64) {
        self.sessions.retain(|s| now.saturating_sub(s.at) < TIMEOUT_TICKS);
    }

    pub fn current(&self) -> Option<Cc> {
        self.sessions.iter().map(|s| s.state).max_by_key(|s| s.priority())
    }

    pub fn outranks(&self, flash: Cc) -> bool {
        self.current().map(|c| c.priority() > flash.priority()).unwrap_or(false)
    }
}

pub fn settings_path() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".claude").join("settings.json"))
}

pub fn our_command() -> Option<String> {
    std::env::current_exe().ok().map(|e| format!("\"{}\" --hook", e.display()))
}

fn is_ours(group: &Value) -> bool {
    group["hooks"].as_array().map_or(false, |hs| {
        hs.iter().any(|h| {
            h["command"]
                .as_str()
                .map_or(false, |c| c.ends_with(" --hook") && c.to_lowercase().contains("claude_crab"))
        })
    })
}

pub fn installed(root: &Value) -> bool {
    EVENTS
        .iter()
        .any(|e| root["hooks"][e].as_array().map_or(false, |a| a.iter().any(is_ours)))
}

fn handler(cmd: &str) -> Value {
    json!({ "hooks": [{ "type": "command", "command": cmd, "async": true, "timeout": 5 }] })
}

pub fn merge(root: &mut Value, cmd: Option<&str>) {
    if !root.is_object() {
        *root = json!({});
    }
    if !root["hooks"].is_object() {
        if cmd.is_none() {
            return;
        }
        root["hooks"] = json!({});
    }
    let hooks = root["hooks"].as_object_mut().unwrap();
    for e in EVENTS {
        if let Some(arr) = hooks.get_mut(e).and_then(|a| a.as_array_mut()) {
            arr.retain(|g| !is_ours(g));
        }
        if let Some(c) = cmd {
            let arr = hooks.entry(e).or_insert_with(|| json!([]));
            if !arr.is_array() {
                *arr = json!([]);
            }
            arr.as_array_mut().unwrap().push(handler(c));
        } else if hooks.get(e).and_then(|a| a.as_array()).map_or(false, |a| a.is_empty()) {
            hooks.remove(e);
        }
    }
    if hooks.is_empty() {
        root.as_object_mut().unwrap().remove("hooks");
    }
}

pub fn read_settings() -> Result<Value, String> {
    let path = settings_path().ok_or("USERPROFILE not set")?;
    match std::fs::read_to_string(&path) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("settings.json: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

pub fn is_connected() -> bool {
    read_settings().map(|v| installed(&v)).unwrap_or(false)
}

pub fn connect(on: bool) -> Result<(), String> {
    let path = settings_path().ok_or("USERPROFILE not set")?;
    let mut root = read_settings()?;
    let cmd = our_command().ok_or("exe path unknown")?;
    if path.exists() {
        std::fs::copy(&path, path.with_extension("json.crab-backup")).map_err(|e| e.to_string())?;
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    merge(&mut root, on.then_some(cmd.as_str()));
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.crab-tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_hook_events() {
        let m = |v: Value| hook_command(&v);
        assert_eq!(m(json!({"hook_event_name": "PreToolUse", "tool_name": "Edit", "session_id": "ab-1"})).as_deref(), Some("cc:edit:ab-1"));
        assert_eq!(m(json!({"hook_event_name": "PreToolUse", "tool_name": "Grep"})).as_deref(), Some("cc:read:-"));
        assert_eq!(m(json!({"hook_event_name": "Notification", "notification_type": "permission_prompt"})).as_deref(), Some("cc:ask:-"));
        assert_eq!(m(json!({"hook_event_name": "Notification", "notification_type": "idle_prompt"})), None);
        assert_eq!(m(json!({"hook_event_name": "StopFailure", "session_id": "x:y"})).as_deref(), Some("cc:fail:xy"));
        assert_eq!(m(json!({"tool_name": "Edit"})), None);
    }

    #[test]
    fn merge_keeps_foreign_hooks_and_is_idempotent() {
        let foreign = json!({"matcher": "Bash", "hooks": [{"type": "command", "command": "lint.sh"}]});
        let mut root = json!({"model": "x", "hooks": {"PreToolUse": [foreign.clone()]}});
        let cmd = "\"C:\\a\\claude_crab.exe\" --hook";
        merge(&mut root, Some(cmd));
        merge(&mut root, Some(cmd));
        assert!(installed(&root));
        assert_eq!(root["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
        merge(&mut root, None);
        assert!(!installed(&root));
        assert_eq!(root, json!({"model": "x", "hooks": {"PreToolUse": [foreign]}}));
        let mut empty = json!({});
        merge(&mut empty, Some(cmd));
        merge(&mut empty, None);
        assert_eq!(empty, json!({}));
    }

    #[test]
    fn tracker_priority_and_timeout() {
        let mut t = Tracker::default();
        t.event(Cc::Think, "a", 0);
        t.event(Cc::Work(Act::Edit), "b", 0);
        assert_eq!(t.current(), Some(Cc::Work(Act::Edit)));
        t.event(Cc::Ask, "a", 10);
        assert_eq!(t.current(), Some(Cc::Ask));
        assert!(t.outranks(Cc::Fail));
        t.event(Cc::Done, "a", 20);
        assert_eq!(t.current(), Some(Cc::Work(Act::Edit)));
        assert!(!t.outranks(Cc::Fail));
        t.expire(TIMEOUT_TICKS);
        assert_eq!(t.current(), None);
    }

    #[test]
    fn test_events_mute_real_sessions() {
        let mut t = Tracker::default();
        t.event(Cc::Work(Act::Bash), "real", 0);
        assert!(t.event(Cc::Think, "test", 10));
        assert_eq!(t.current(), Some(Cc::Think));
        assert!(!t.event(Cc::Ask, "real", 20));
        assert_eq!(t.current(), Some(Cc::Think));
        assert!(t.event(Cc::Ask, "real", 10 + TEST_TICKS));
        assert_eq!(t.current(), Some(Cc::Ask));
    }
}
