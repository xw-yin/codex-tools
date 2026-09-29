//! macOS application bundle discovery.
use super::*;
const MACOS_CODEX_APP_NAMES: [&str; 3] = ["ChatGPT.app", "Codex.app", "Codex Desktop.app"];

#[cfg(target_os = "macos")]
pub(super) fn macos_codex_app_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // 新版 Codex 桌面端使用 ChatGPT.app 名称；旧名称继续作为兼容回退。
    for app_name in MACOS_CODEX_APP_NAMES {
        candidates.push(Path::new("/Applications").join(app_name));
        if let Some(home) = home {
            candidates.push(home.join("Applications").join(app_name));
        }
    }

    candidates
}

#[cfg(target_os = "macos")]
pub(crate) fn is_macos_codex_app_bundle(path: &Path) -> bool {
    if !is_macos_app_bundle(path) {
        return false;
    }

    let Some(app_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    if matches_ignore_ascii_case(app_name, &["Codex.app", "Codex Desktop.app"]) {
        return true;
    }
    if !app_name.eq_ignore_ascii_case("ChatGPT.app") {
        return false;
    }

    // ChatGPT.app 历史上也可能是普通聊天客户端，内置 codex 才能证明它支持当前启动协议。
    let resources = path.join("Contents").join("Resources");
    is_executable_file(&resources.join("codex"))
        || is_executable_file(&resources.join("codex-cli").join("bin").join("codex"))
        || is_executable_file(&resources.join("codex-cli").join("CodexCLI.app").join("Contents").join("MacOS").join("codex"))
        || resources.join("codex-cli").is_dir()
        || resources.join("com.openai.codex.manifest").exists()
        || path.join("Contents").join("Frameworks").join("Codex Framework.framework").exists()
}

#[cfg(target_os = "macos")]
pub(super) fn append_macos_app_bundle_codex_candidates(candidates: &mut Vec<PathBuf>) {
    let home = dirs::home_dir();
    let mut app_paths = macos_codex_app_candidates(home.as_deref());

    if let Some(found) = find_codex_app_path() {
        app_paths.push(found);
    }

    for app_path in app_paths {
        let resources = app_path.join("Contents").join("Resources");
        candidates.push(resources.join("codex"));
        candidates.push(resources.join("codex-cli").join("bin").join("codex"));
        candidates.push(resources.join("codex-cli").join("CodexCLI.app").join("Contents").join("MacOS").join("codex"));
    }
}

#[cfg(target_os = "macos")]
pub(super) fn first_spotlight_codex_app_match(query: &str) -> Option<PathBuf> {
    let output = Command::new("mdfind").arg(query).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .find(|path| is_macos_codex_app_bundle(path))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::is_macos_codex_app_bundle;
    use super::macos_codex_app_candidates;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn macos_candidates_prioritize_chatgpt_and_keep_legacy_names() {
        let candidates = macos_codex_app_candidates(Some(Path::new("/Users/tester")));

        assert_eq!(
            candidates,
            vec![
                Path::new("/Applications/ChatGPT.app").to_path_buf(),
                Path::new("/Users/tester/Applications/ChatGPT.app").to_path_buf(),
                Path::new("/Applications/Codex.app").to_path_buf(),
                Path::new("/Users/tester/Applications/Codex.app").to_path_buf(),
                Path::new("/Applications/Codex Desktop.app").to_path_buf(),
                Path::new("/Users/tester/Applications/Codex Desktop.app").to_path_buf(),
            ]
        );
    }

    #[test]
    fn chatgpt_candidate_requires_embedded_codex_but_legacy_bundle_stays_compatible() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let sandbox = std::env::temp_dir().join(format!(
            "codex-tools-cli-test-{}-{nonce}",
            std::process::id()
        ));
        let chatgpt_app = sandbox.join("ChatGPT.app");
        let legacy_app = sandbox.join("Codex.app");
        fs::create_dir_all(chatgpt_app.join("Contents").join("Resources"))
            .expect("create ChatGPT test bundle");
        fs::create_dir_all(&legacy_app).expect("create legacy Codex test bundle");

        assert!(!is_macos_codex_app_bundle(&chatgpt_app));
        assert!(is_macos_codex_app_bundle(&legacy_app));

        let embedded_codex = chatgpt_app.join("Contents").join("Resources").join("codex");
        fs::write(&embedded_codex, b"test").expect("write embedded codex marker");
        let mut permissions = fs::metadata(&embedded_codex)
            .expect("read marker metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&embedded_codex, permissions).expect("make marker executable");

        assert!(is_macos_codex_app_bundle(&chatgpt_app));
        let _ = fs::remove_dir_all(sandbox);
    }

    #[test]
    fn chatgpt_candidate_accepts_modern_codex_cli_bundle() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        let sandbox = std::env::temp_dir().join(format!(
            "codex-tools-cli-test-modern-{}-{nonce}",
            std::process::id()
        ));
        let chatgpt_app = sandbox.join("ChatGPT.app");
        let codex_cli_bin = chatgpt_app
            .join("Contents")
            .join("Resources")
            .join("codex-cli")
            .join("bin");
        fs::create_dir_all(&codex_cli_bin).expect("create modern ChatGPT codex-cli bundle");

        let embedded_codex = codex_cli_bin.join("codex");
        fs::write(&embedded_codex, b"test").expect("write embedded modern codex marker");
        let mut permissions = fs::metadata(&embedded_codex)
            .expect("read marker metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&embedded_codex, permissions).expect("make marker executable");

        assert!(is_macos_codex_app_bundle(&chatgpt_app));
        let _ = fs::remove_dir_all(sandbox);
    }
}
