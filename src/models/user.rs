use serde::{Deserialize, Serialize};

pub const COMMON_ROLES: &[&str] = &[
    "read",
    "readWrite",
    "dbAdmin",
    "userAdmin",
    "readAnyDatabase",
    "readWriteAnyDatabase",
    "userAdminAnyDatabase",
    "dbAdminAnyDatabase",
    "clusterAdmin",
    "root",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRole {
    pub role: String,
    pub db: String,
}

impl UserRole {
    pub const COMMON_ROLES: &[&str] = COMMON_ROLES;

    #[must_use]
    pub fn new(role: &str, db: &str) -> Self {
        Self {
            role: role.trim().to_string(),
            db: db.trim().to_string(),
        }
    }

    #[must_use]
    pub fn description(&self) -> &'static str {
        Self::role_description(&self.role)
    }

    #[must_use]
    pub fn role_description(role: &str) -> &'static str {
        match role {
            "read" => "Read data on all non-system collections in the target database",
            "readWrite" => {
                "Read and modify data on all non-system collections in the target database"
            }
            "dbAdmin" => "Schema, indexing, and statistics administration on the target database",
            "userAdmin" => "Create and modify users and roles on the target database",
            "readAnyDatabase" => "Read data on all databases across the cluster",
            "readWriteAnyDatabase" => "Read and write data on all databases across the cluster",
            "userAdminAnyDatabase" => "Manage users and roles across all databases",
            "dbAdminAnyDatabase" => {
                "Administer collections, indexes, and stats across all databases"
            }
            "clusterAdmin" => "Full cluster, replica set, and sharding administration privileges",
            "root" => "Superuser access to all resources and operations",
            _ => "Custom or built-in MongoDB role on target database",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MongoUser {
    pub id: String,
    pub username: String,
    pub db: String,
    pub roles: Vec<UserRole>,
}

impl MongoUser {
    #[must_use]
    pub fn is_protected_admin(&self) -> bool {
        self.username.trim().eq_ignore_ascii_case("admin")
    }

    #[must_use]
    pub fn mock_users() -> Vec<Self> {
        vec![
            Self {
                id: "admin.admin".to_string(),
                username: "admin".to_string(),
                db: "admin".to_string(),
                roles: vec![
                    UserRole::new("root", "admin"),
                    UserRole::new("userAdminAnyDatabase", "admin"),
                ],
            },
            Self {
                id: "admin.app_service".to_string(),
                username: "app_service".to_string(),
                db: "admin".to_string(),
                roles: vec![
                    UserRole::new("readWrite", "shop_db"),
                    UserRole::new("read", "admin"),
                ],
            },
        ]
    }
}
