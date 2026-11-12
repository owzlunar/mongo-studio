use egui_phosphor::regular;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BottomPanelTab {
    Terminal,
    Output,
    QueryHistory,
    Problems,
}

impl BottomPanelTab {
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Terminal => regular::TERMINAL_WINDOW,
            Self::Output => regular::LIST_BULLETS,
            Self::QueryHistory => regular::CLOCK_COUNTER_CLOCKWISE,
            Self::Problems => regular::WARNING_CIRCLE,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Terminal => "TERMINAL",
            Self::Output => "OUTPUT",
            Self::QueryHistory => "QUERY HISTORY",
            Self::Problems => "PROBLEMS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottomPanelState {
    pub visible: bool,
    pub active_tab: BottomPanelTab,
    pub terminal_input: String,
    pub terminal_history: Vec<String>,
    pub logs: Vec<LogEntry>,
    pub query_history: Vec<String>,
}

impl Default for BottomPanelState {
    fn default() -> Self {
        Self {
            visible: true,
            active_tab: BottomPanelTab::Output,
            terminal_input: String::new(),
            terminal_history: vec![
                "Mongo Studio Shell (Mock Mode — Phase 1)".to_string(),
                "Connected to mongodb://localhost:27017 (MongoDB 7.0)".to_string(),
                "Type `db.users.find({\"status\": \"active\"})` or `help`".to_string(),
            ],
            logs: vec![
                LogEntry {
                    timestamp: "13:00:01".to_string(),
                    level: LogLevel::Info,
                    source: "App".to_string(),
                    message: "Initialized Mongo Studio UI Shell (Phase 1)".to_string(),
                },
                LogEntry {
                    timestamp: "13:00:02".to_string(),
                    level: LogLevel::Success,
                    source: "ConnectionManager".to_string(),
                    message: "Loaded mock profile: MongoDB Local (localhost:27017)".to_string(),
                },
                LogEntry {
                    timestamp: "13:00:03".to_string(),
                    level: LogLevel::Info,
                    source: "QueryRunner".to_string(),
                    message: "admin.users.find({\"status\": \"active\"}) -> returned 2 active documents (1.2 ms)".to_string(),
                },
            ],
            query_history: vec![
                "db.getSiblingDB(\"admin\").users.find({\"status\": \"active\"})".to_string(),
                "db.getSiblingDB(\"admin\").products.find({})".to_string(),
                "db.getSiblingDB(\"shop_db\").customers.find({\"role\": \"admin\"})".to_string(),
            ],
        }
    }
}

impl BottomPanelState {
    pub fn push_log(&mut self, level: LogLevel, source: &str, message: impl Into<String>) {
        let count = self.logs.len() + 1;
        self.logs.push(LogEntry {
            timestamp: format!("00:{:02}:{:02}", (count / 60) % 60, count % 60),
            level,
            source: source.to_string(),
            message: message.into(),
        });
    }
}
