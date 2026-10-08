use anyhow::{Context, Result};
use schemars::JsonSchema;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LaunchableApp {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    path: PathBuf,
}

fn collect(directory: &Path, root: &Path, entries: &mut BTreeMap<String, Option<LaunchableApp>>) {
    let Ok(children) = std::fs::read_dir(directory) else {
        return;
    };
    for child in children.flatten() {
        let path = child.path();
        if child.file_type().is_ok_and(|kind| kind.is_dir()) {
            collect(&path, root, entries);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "desktop") {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let id = relative.to_string_lossy().replace('/', "-");
        if entries.contains_key(&id) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut section = false;
        let mut fields = BTreeMap::new();
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                section = line == "[Desktop Entry]";
            } else if section && !line.starts_with('#') {
                if let Some((key, value)) = line.split_once('=') {
                    fields.insert(key, value);
                }
            }
        }
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
        let matches_desktop = |list: &str| {
            list.split(';')
                .filter(|s| !s.is_empty())
                .any(|entry| desktop.split(':').any(|d| d == entry))
        };
        let visible = fields.get("Type") == Some(&"Application")
            && fields.get("Hidden") != Some(&"true")
            && fields.get("NoDisplay") != Some(&"true")
            && fields
                .get("OnlyShowIn")
                .is_none_or(|list| matches_desktop(list))
            && fields
                .get("NotShowIn")
                .is_none_or(|list| !matches_desktop(list))
            && (fields.contains_key("Exec") || fields.get("DBusActivatable") == Some(&"true"));
        let app = fields
            .get("Name")
            .filter(|_| visible)
            .map(|name| LaunchableApp {
                id: id.clone(),
                name: (*name).into(),
                path,
            });
        entries.insert(id, app);
    }
}

pub fn list_launchable_apps() -> Vec<LaunchableApp> {
    let user = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share")
        });
    let system =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut entries = BTreeMap::new();
    for directory in std::iter::once(user).chain(std::env::split_paths(&system)) {
        let root = directory.join("applications");
        collect(&root, &root, &mut entries);
    }
    let mut apps: Vec<_> = entries.into_values().flatten().collect();
    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps
}

pub async fn launch(app_id: &str) -> Result<()> {
    let app = list_launchable_apps()
        .into_iter()
        .find(|app| app.id == app_id)
        .with_context(|| {
            format!("Application {app_id:?} was not found. Choose an id from list_launchable_apps.")
        })?;
    let mut command = tokio::process::Command::new("gio");
    command.arg("launch").arg(app.path);
    // Launched apps inherit stdio; captured pipes would keep a launch request open for the app's lifetime.
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), command.status())
        .await
        .context("Desktop launcher did not return within 10 seconds.")??;
    anyhow::ensure!(
        status.success(),
        "Desktop launcher exited with {status} for {app_id}."
    );
    Ok(())
}
