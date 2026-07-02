use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn plugins_dir() -> String {
    std::env::var("CMX_PLUGINS_DIR").unwrap_or_else(|_| "./published_plugins".to_string())
}

pub fn find_plugin_dir_by_id(plugin_id: &str) -> Option<PathBuf> {
    let plugins_directory = plugins_dir();
    let dir = Path::new(&plugins_directory);
    if !dir.is_dir() {
        return None;
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some((id, _, _)) = get_plugin_info_from_manifest(&path)
            && id == plugin_id
        {
            return Some(path);
        }
    }
    None
}

pub fn find_plugin_dir_by_name(name: &str) -> PathBuf {
    let plugins_dir = plugins_dir();
    Path::new(&plugins_dir).join(name)
}

pub fn find_wasm_file(dir: &Path) -> Option<PathBuf> {
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension() == Some("wasm".as_ref()) {
            return Some(path.to_path_buf());
        }
    }
    None
}

pub fn read_plugin_json(dir: &Path) -> Option<serde_json::Value> {
    if let Ok(content) = std::fs::read_to_string(dir.join("manifest.json"))
        && let Ok(json) = serde_json::from_str(&content)
    {
        return Some(json);
    }
    if let Ok(content) = std::fs::read_to_string(dir.join("cmx-plugin.json"))
        && let Ok(json) = serde_json::from_str(&content)
    {
        return Some(json);
    }
    None
}

pub fn get_source_path_from_plugin_json(dir: &Path) -> Option<String> {
    let json = read_plugin_json(dir)?;
    let plugin = json.get("plugin")?;
    let source = plugin.get("source_path")?.as_str()?;
    let source_dir = dir.join(source);
    Some(source_dir.to_string_lossy().to_string())
}

pub fn get_plugin_info_from_json(dir: &Path) -> Option<(String, String)> {
    let json = read_plugin_json(dir)?;
    let plugin = json.get("plugin")?;
    let name = plugin.get("name")?.as_str()?.to_string();
    let version = plugin.get("version")?.as_str()?.to_string();
    Some((name, version))
}

fn find_manifest_file(dir: &Path) -> Option<PathBuf> {
    let manifest_path = dir.join("manifest.json");
    if manifest_path.exists() {
        return Some(manifest_path);
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(found) = find_manifest_file(&path)
        {
            return Some(found);
        }
    }
    None
}

pub fn get_plugin_info_from_manifest(dir: &Path) -> Option<(String, String, String)> {
    let manifest_path = find_manifest_file(dir)?;
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let plugin = json.get("plugin")?;
    let id = plugin.get("id")?.as_str()?.to_string();
    let name = plugin.get("name")?.as_str()?.to_string();
    let source_path = plugin.get("source_path")?.as_str()?.to_string();
    Some((id, name, source_path))
}

pub fn find_wit_file(dir: &Path) -> Option<PathBuf> {
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension() == Some("wit".as_ref()) {
            return Some(path.to_path_buf());
        }
    }
    None
}
