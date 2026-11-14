use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use super::connection::ConnectionConfig;

/// Environment tag representing deployment environments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnvironmentTag {
    Dev,
    Test,
    Prod,
    Other(String),
}

impl EnvironmentTag {
    /// Returns the canonical string representation of the environment tag.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Dev => "DEV",
            Self::Test => "TEST",
            Self::Prod => "PROD",
            Self::Other(custom) => custom.as_str(),
        }
    }

    /// User-facing label for badges and dropdowns.
    #[must_use]
    pub fn label(&self) -> &str {
        self.as_str()
    }

    /// Automatically detects an environment tag based on connection name patterns.
    #[must_use]
    pub fn detect_from_name(name: &str) -> Option<Self> {
        let lower = name.trim().to_lowercase();
        if lower.is_empty() {
            return None;
        }

        if lower.ends_with("-prod")
            || lower.ends_with("_prod")
            || lower.ends_with("-production")
            || lower.ends_with("_production")
            || lower.contains("production")
        {
            Some(Self::Prod)
        } else if lower.ends_with("-test")
            || lower.ends_with("_test")
            || lower.ends_with("-staging")
            || lower.ends_with("_staging")
            || lower.ends_with("-stg")
            || lower.ends_with("_stg")
            || lower.ends_with("-uat")
            || lower.ends_with("_uat")
            || lower.ends_with("-qa")
            || lower.ends_with("_qa")
        {
            Some(Self::Test)
        } else if lower.ends_with("-dev")
            || lower.ends_with("_dev")
            || lower.ends_with("-development")
            || lower.ends_with("_development")
            || lower.ends_with("-local")
            || lower.ends_with("_local")
        {
            Some(Self::Dev)
        } else {
            None
        }
    }

    /// Returns `(background, text, border)` colors for UI environment badge rendering.
    #[must_use]
    pub fn badge_colors(&self, is_dark: bool) -> (egui::Color32, egui::Color32, egui::Color32) {
        if is_dark {
            match self {
                Self::Dev => (
                    egui::Color32::from_rgba_unmultiplied(6, 78, 59, 200),
                    egui::Color32::from_rgb(52, 211, 153), // #34d399
                    egui::Color32::from_rgba_unmultiplied(4, 120, 87, 180),
                ),
                Self::Test => (
                    egui::Color32::from_rgba_unmultiplied(120, 53, 15, 200),
                    egui::Color32::from_rgb(251, 191, 36), // #fbbf24
                    egui::Color32::from_rgba_unmultiplied(180, 83, 9, 180),
                ),
                Self::Prod => (
                    egui::Color32::from_rgba_unmultiplied(127, 29, 29, 200),
                    egui::Color32::from_rgb(248, 113, 113), // #f87171
                    egui::Color32::from_rgba_unmultiplied(185, 28, 28, 180),
                ),
                Self::Other(_) => (
                    egui::Color32::from_rgba_unmultiplied(51, 65, 85, 200),
                    egui::Color32::from_rgb(203, 213, 225), // #cbd5e1
                    egui::Color32::from_rgba_unmultiplied(100, 116, 139, 180),
                ),
            }
        } else {
            match self {
                Self::Dev => (
                    egui::Color32::from_rgba_unmultiplied(209, 250, 229, 220), // emerald-100
                    egui::Color32::from_rgb(6, 95, 70),                        // emerald-800
                    egui::Color32::from_rgba_unmultiplied(110, 231, 183, 200), // emerald-300
                ),
                Self::Test => (
                    egui::Color32::from_rgba_unmultiplied(254, 243, 199, 220), // amber-100
                    egui::Color32::from_rgb(146, 64, 14),                      // amber-800
                    egui::Color32::from_rgba_unmultiplied(252, 211, 77, 200),  // amber-300
                ),
                Self::Prod => (
                    egui::Color32::from_rgba_unmultiplied(254, 226, 226, 220), // red-100
                    egui::Color32::from_rgb(153, 27, 27),                      // red-800
                    egui::Color32::from_rgba_unmultiplied(252, 165, 165, 200), // red-300
                ),
                Self::Other(_) => (
                    egui::Color32::from_rgba_unmultiplied(241, 245, 249, 220), // slate-100
                    egui::Color32::from_rgb(51, 65, 85),                       // slate-700
                    egui::Color32::from_rgba_unmultiplied(203, 213, 225, 200), // slate-300
                ),
            }
        }
    }
}

impl fmt::Display for EnvironmentTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

impl Serialize for EnvironmentTag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EnvironmentTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim();
        match trimmed.to_uppercase().as_str() {
            "DEV" => Ok(Self::Dev),
            "TEST" => Ok(Self::Test),
            "PROD" => Ok(Self::Prod),
            _ => Ok(Self::Other(trimmed.to_string())),
        }
    }
}

/// A named folder inside a workspace grouping related MongoDB connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFolder {
    pub name: String,
    pub connection_ids: Vec<Uuid>,
}

impl ProjectFolder {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connection_ids: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_connections(name: impl Into<String>, connection_ids: Vec<Uuid>) -> Self {
        Self {
            name: name.into(),
            connection_ids,
        }
    }
}

/// A named workspace item grouping folders and standalone connections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceItem {
    pub name: String,
    pub folders: Vec<ProjectFolder>,
    pub standalone_connections: Vec<Uuid>,
}

impl WorkspaceItem {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            folders: Vec::new(),
            standalone_connections: Vec::new(),
        }
    }

    #[must_use]
    pub fn total_connections(&self) -> usize {
        self.standalone_connections.len()
            + self
                .folders
                .iter()
                .map(|f| f.connection_ids.len())
                .sum::<usize>()
    }
}

type FolderConnectionMap = BTreeMap<String, Vec<Uuid>>;
type WorkspaceGrouping = (Vec<Uuid>, FolderConnectionMap);
type WorkspaceGroupMap = BTreeMap<String, WorkspaceGrouping>;
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTree {
    pub workspaces: Vec<WorkspaceItem>,
    pub unassigned_connections: Vec<Uuid>,
}

impl WorkspaceTree {
    /// Builds an organized `WorkspaceTree` from a slice of connections.
    ///
    /// - Groups connections by workspace and folder.
    /// - Filters connections by `search_filter` (case-insensitive name or environment tag).
    /// - Filters by `active_workspace` (if `Some` and not empty / "All").
    /// - Workspaces and folders are sorted alphabetically for deterministic ordering.
    #[must_use]
    pub fn build(
        connections: &[ConnectionConfig],
        search_filter: &str,
        active_workspace: Option<&str>,
    ) -> Self {
        let filter = search_filter.trim().to_lowercase();
        let active_filter = active_workspace
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all"));

        // workspace_name -> (Vec<standalone_uuid>, BTreeMap<folder_name, Vec<Uuid>>)
        let mut ws_map: WorkspaceGroupMap = BTreeMap::new();
        let mut unassigned = Vec::new();

        for conn in connections {
            let matches_search = if filter.is_empty() {
                true
            } else {
                conn.name.to_lowercase().contains(&filter)
                    || conn
                        .effective_environment()
                        .is_some_and(|env| env.label().to_lowercase().contains(&filter))
                    || conn.host.to_lowercase().contains(&filter)
            };

            if !matches_search {
                continue;
            }

            if let Some(ws) = conn.workspace_name() {
                if let Some(active) = active_filter {
                    if !ws.eq_ignore_ascii_case(active) {
                        continue;
                    }
                }

                let entry = ws_map.entry(ws.to_string()).or_default();
                if let Some(folder) = conn.folder_name() {
                    entry.1.entry(folder.to_string()).or_default().push(conn.id);
                } else {
                    entry.0.push(conn.id);
                }
            } else if active_filter.is_none() {
                unassigned.push(conn.id);
            }
        }

        let mut workspaces = Vec::with_capacity(ws_map.len());
        for (ws_name, (standalone, folders_map)) in ws_map {
            let mut folders = Vec::with_capacity(folders_map.len());
            for (folder_name, conn_ids) in folders_map {
                folders.push(ProjectFolder {
                    name: folder_name,
                    connection_ids: conn_ids,
                });
            }
            workspaces.push(WorkspaceItem {
                name: ws_name,
                folders,
                standalone_connections: standalone,
            });
        }

        Self {
            workspaces,
            unassigned_connections: unassigned,
        }
    }

    /// Returns a unique, sorted list of workspace names across all connections.
    #[must_use]
    pub fn extract_all_workspaces(connections: &[ConnectionConfig]) -> Vec<String> {
        let mut workspaces: Vec<String> = connections
            .iter()
            .filter_map(|c| c.workspace_name().map(ToString::to_string))
            .collect();
        workspaces.sort_unstable_by_key(|a| a.to_lowercase());
        workspaces.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        workspaces
    }

    /// Returns a unique, sorted list of folder names for a specific workspace.
    #[must_use]
    pub fn extract_folders_for_workspace(
        connections: &[ConnectionConfig],
        workspace: &str,
    ) -> Vec<String> {
        let target = workspace.trim();
        if target.is_empty() {
            return Vec::new();
        }

        let mut folders: Vec<String> = connections
            .iter()
            .filter(|c| {
                c.workspace_name()
                    .is_some_and(|ws| ws.eq_ignore_ascii_case(target))
            })
            .filter_map(|c| c.folder_name().map(ToString::to_string))
            .collect();
        folders.sort_unstable_by_key(|a| a.to_lowercase());
        folders.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        folders
    }

    /// Returns the total count of connections across all workspaces and unassigned.
    #[must_use]
    pub fn total_connections(&self) -> usize {
        self.unassigned_connections.len()
            + self
                .workspaces
                .iter()
                .map(WorkspaceItem::total_connections)
                .sum::<usize>()
    }

    /// Returns `true` if the tree contains no workspaces and no unassigned connections.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.workspaces.is_empty() && self.unassigned_connections.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_tag_serialization_and_display() {
        assert_eq!(EnvironmentTag::Dev.as_str(), "DEV");
        assert_eq!(EnvironmentTag::Test.as_str(), "TEST");
        assert_eq!(EnvironmentTag::Prod.as_str(), "PROD");
        assert_eq!(EnvironmentTag::Other("Staging".into()).as_str(), "Staging");

        assert_eq!(format!("{}", EnvironmentTag::Dev), "DEV");
        assert_eq!(format!("{}", EnvironmentTag::Other("QA-1".into())), "QA-1");

        let serialized = serde_json::to_string(&EnvironmentTag::Prod).expect("serialize tag");
        assert_eq!(serialized, "\"PROD\"");

        let deserialized: EnvironmentTag =
            serde_json::from_str("\"dev\"").expect("deserialize dev");
        assert_eq!(deserialized, EnvironmentTag::Dev);

        let deserialized_custom: EnvironmentTag =
            serde_json::from_str("\"Staging\"").expect("deserialize staging");
        assert_eq!(deserialized_custom, EnvironmentTag::Other("Staging".into()));
    }

    #[test]
    fn detect_from_name_matches_patterns() {
        // Prod
        assert_eq!(
            EnvironmentTag::detect_from_name("mongo-prod"),
            Some(EnvironmentTag::Prod)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("db_prod"),
            Some(EnvironmentTag::Prod)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("api-production"),
            Some(EnvironmentTag::Prod)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("app_production"),
            Some(EnvironmentTag::Prod)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("production-cluster"),
            Some(EnvironmentTag::Prod)
        );

        // Test
        assert_eq!(
            EnvironmentTag::detect_from_name("mongo-test"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("db_test"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("api-staging"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("app_staging"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("cluster-stg"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("db_stg"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("api-uat"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("app_uat"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("cluster-qa"),
            Some(EnvironmentTag::Test)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("db_qa"),
            Some(EnvironmentTag::Test)
        );

        // Dev
        assert_eq!(
            EnvironmentTag::detect_from_name("mongo-dev"),
            Some(EnvironmentTag::Dev)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("db_dev"),
            Some(EnvironmentTag::Dev)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("api-development"),
            Some(EnvironmentTag::Dev)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("app_development"),
            Some(EnvironmentTag::Dev)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("mongo-local"),
            Some(EnvironmentTag::Dev)
        );
        assert_eq!(
            EnvironmentTag::detect_from_name("db_local"),
            Some(EnvironmentTag::Dev)
        );

        // None
        assert_eq!(EnvironmentTag::detect_from_name("my-database"), None);
        assert_eq!(EnvironmentTag::detect_from_name(""), None);
    }

    #[test]
    fn badge_colors_return_valid_palettes() {
        for is_dark in [true, false] {
            let (bg, text, border) = EnvironmentTag::Dev.badge_colors(is_dark);
            assert_ne!(bg, text);
            assert_ne!(text, border);

            let (bg_p, text_p, _) = EnvironmentTag::Prod.badge_colors(is_dark);
            assert_ne!(bg_p, text_p);

            let (bg_t, text_t, _) = EnvironmentTag::Test.badge_colors(is_dark);
            assert_ne!(bg_t, text_t);

            let (bg_o, text_o, _) = EnvironmentTag::Other("Custom".into()).badge_colors(is_dark);
            assert_ne!(bg_o, text_o);
        }
    }

    #[test]
    fn workspace_tree_build_and_extraction() {
        let mut c1 = ConnectionConfig::new_profile("app-prod", "mongodb://localhost:27017")
            .expect("profile c1");
        c1.workspace = Some("Finance".to_string());
        c1.folder = Some("Backend".to_string());

        let mut c2 = ConnectionConfig::new_profile("app-dev", "mongodb://localhost:27018")
            .expect("profile c2");
        c2.workspace = Some("Finance".to_string());
        c2.folder = None; // standalone

        let mut c3 =
            ConnectionConfig::new_profile("analytics-staging", "mongodb://localhost:27019")
                .expect("profile c3");
        c3.workspace = Some("Data".to_string());
        c3.folder = Some("Pipelines".to_string());

        let c4 = ConnectionConfig::new_profile("scratchpad", "mongodb://localhost:27020")
            .expect("profile c4"); // unassigned

        let connections = vec![c1.clone(), c2.clone(), c3.clone(), c4.clone()];

        // 1. All workspaces tree
        let tree = WorkspaceTree::build(&connections, "", None);
        assert_eq!(tree.workspaces.len(), 2);
        assert_eq!(tree.workspaces[0].name, "Data");
        assert_eq!(tree.workspaces[0].folders.len(), 1);
        assert_eq!(tree.workspaces[0].folders[0].name, "Pipelines");
        assert_eq!(tree.workspaces[0].folders[0].connection_ids, vec![c3.id]);
        assert_eq!(tree.workspaces[0].standalone_connections.len(), 0);

        assert_eq!(tree.workspaces[1].name, "Finance");
        assert_eq!(tree.workspaces[1].folders.len(), 1);
        assert_eq!(tree.workspaces[1].folders[0].name, "Backend");
        assert_eq!(tree.workspaces[1].folders[0].connection_ids, vec![c1.id]);
        assert_eq!(tree.workspaces[1].standalone_connections, vec![c2.id]);
        assert_eq!(tree.unassigned_connections, vec![c4.id]);
        assert_eq!(tree.total_connections(), 4);

        // 2. Active workspace filter
        let filtered_tree = WorkspaceTree::build(&connections, "", Some("Finance"));
        assert_eq!(filtered_tree.workspaces.len(), 1);
        assert_eq!(filtered_tree.workspaces[0].name, "Finance");
        assert!(filtered_tree.unassigned_connections.is_empty());

        // 3. Search filter by tag
        let prod_tree = WorkspaceTree::build(&connections, "prod", None);
        assert_eq!(prod_tree.total_connections(), 1);
        assert_eq!(
            prod_tree.workspaces[0].folders[0].connection_ids,
            vec![c1.id]
        );

        // 4. Extraction helpers
        let all_ws = WorkspaceTree::extract_all_workspaces(&connections);
        assert_eq!(all_ws, vec!["Data", "Finance"]);

        let finance_folders = WorkspaceTree::extract_folders_for_workspace(&connections, "Finance");
        assert_eq!(finance_folders, vec!["Backend"]);

        let empty_folders =
            WorkspaceTree::extract_folders_for_workspace(&connections, "NonExistent");
        assert!(empty_folders.is_empty());
    }
}
