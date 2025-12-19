mod config;
mod editor;
mod git;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::MetaConfig;
use editor::CrateEditor;
use glob::glob;
use semver::Version;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

#[derive(Parser)]
#[command(name = "meta")]
#[command(about = "Manage a meta-workspace of Rust crates", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Bump the version of all crates in the meta-workspace
    Bump {
        /// The new version to set (e.g. "0.2.0")
        version: Version,
    },
    /// Initialize a new Meta.toml by scanning the current directory
    Init,
    /// Create (if needed) and switch to a branch in all repositories
    Branch { name: String },
    /// Checkout a branch in all repositories
    Checkout { name: String },
    /// Merge a branch into the current branch in all repositories
    Merge { branch: String },
    /// Commit changes with a version bump message in all repositories
    Commit {
        /// Custom commit message (required)
        #[arg(short, long)]
        message: String,
    },
    /// Push changes to remote in all repositories
    Push,
    /// Push the version tag to origin (vX.Y.Z)
    PushTag,
    /// Create a version tag in all repositories
    Tag,
    /// Remove a branch in all repositories
    RemoveBranch {
        name: String,
        #[arg(long)]
        remote: bool,
    },
    /// Remove a tag in all repositories
    RemoveTag {
        name: String,
        #[arg(long)]
        remote: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Bump { version } => bump_all(version),
        Commands::Init => generate_meta(),
        Commands::Branch { name } => run_git_on_all(|repo, _| git::create_branch(repo, &name)),
        Commands::Checkout { name } => run_git_on_all(|repo, _| git::checkout_branch(repo, &name)),
        Commands::Merge { branch } => run_git_on_all(|repo, _| git::merge_branch(repo, &branch)),
        Commands::Commit { message } => run_git_on_all(|repo, members| {
            let files: Vec<PathBuf> = members.iter().map(|m| m.join("Cargo.toml")).collect();
            git::commit(repo, message, &files)
        }),
        Commands::Push => run_git_on_all(|repo, _| git::push(repo)),
        Commands::PushTag => run_git_on_all(|repo, _| git::push_tag(repo)),
        Commands::Tag => run_git_on_all(|repo, _| git::create_tag(repo)),
        Commands::RemoveBranch { name, remote } => {
            run_git_on_all(|repo, _| git::remove_branch(repo, &name, *remote))
        }
        Commands::RemoveTag { name, remote } => {
            run_git_on_all(|repo, _| git::remove_tag(repo, &name, *remote))
        }
    }
}

fn run_git_on_all<F>(op: F) -> Result<()>
where
    F: Fn(&Path, &[PathBuf]) -> Result<()>,
{
    let config = MetaConfig::load()?;
    let member_paths: Vec<PathBuf> = config
        .workspace
        .members
        .iter()
        .map(|m| PathBuf::from(m))
        .collect();

    let repo_map = git::group_members_by_repo(&member_paths)?;

    println!("Found {} unique repositories.", repo_map.len());

    for (repo_root, members) in repo_map {
        if let Err(e) = op(&repo_root, &members) {
            eprintln!("Error in repo {:?}: {}", repo_root, e);
        }
    }
    Ok(())
}

fn generate_meta() -> Result<()> {
    let current_dir = std::env::current_dir()?;
    generate_meta_at(&current_dir)
}

fn generate_meta_at(current_dir: &Path) -> Result<()> {
    // 1. Scan subdirectories
    let mut members = Vec::new();

    println!("Scanning {} for crates...", current_dir.display());

    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let cargo_toml_path = path.join("Cargo.toml");
            if cargo_toml_path.exists() {
                process_crate_or_workspace(&mut members, current_dir, &path, &cargo_toml_path)?;
            }
        }
    }

    // sort members
    members.sort();

    // dedup members
    members.dedup();

    println!("Found {} members: {:?}", members.len(), members);

    if members.is_empty() {
        println!("No crates found. Exiting.");
        return Ok(());
    }

    // 2. Write Meta.toml
    let meta_path = current_dir.join("Meta.toml");
    if meta_path.exists() {
        // For safety, let's not overwrite if it exists without asking (or just fail for now)
        // User requested "generate an initial version", usually implies fresh start.
        // I will fail if exists to be safe.
        anyhow::bail!(
            "Meta.toml already exists. Please delete it or rename it before running init."
        );
    }

    // Create config structure manually or just write toml string
    let mut doc = DocumentMut::new();
    doc["workspace"] = toml_edit::table();

    let mut members_array = toml_edit::Array::new();
    for member in members {
        members_array.push(member);
    }

    doc["workspace"]["members"] = toml_edit::value(members_array);

    fs::write(meta_path, doc.to_string())?;
    println!("Generated Meta.toml successfully.");

    Ok(())
}

fn process_crate_or_workspace(
    members: &mut Vec<String>,
    root_path: &Path,
    dir_path: &Path,
    cargo_toml_path: &Path,
) -> Result<()> {
    let content = fs::read_to_string(cargo_toml_path)?;
    let doc = content.parse::<DocumentMut>()?;

    // Check if it is a workspace
    if let Some(workspace) = doc.get("workspace") {
        if let Some(ws_members) = workspace.get("members").and_then(|m| m.as_array()) {
            for member in ws_members {
                if let Some(member_str) = member.as_str() {
                    // Resolve glob
                    let pattern = dir_path.join(member_str);
                    let pattern_str = pattern.to_string_lossy();

                    for entry in glob(&pattern_str)? {
                        match entry {
                            Ok(p) => {
                                // verify it has a Cargo.toml
                                if p.join("Cargo.toml").exists() {
                                    // Add relative path from root_path
                                    if let Ok(rel) = p.strip_prefix(root_path) {
                                        members.push(rel.to_string_lossy().replace("\\", "/"));
                                    }
                                }
                            }
                            Err(e) => eprintln!("Glob error: {:?}", e),
                        }
                    }
                }
            }
        }
    } else if doc.get("package").is_some() {
        // It's a single crate
        if let Ok(rel) = dir_path.strip_prefix(root_path) {
            members.push(rel.to_string_lossy().replace("\\", "/"));
        }
    }

    Ok(())
}

fn bump_all(new_version: &Version) -> Result<()> {
    let config = MetaConfig::load()?;
    let mut editors = Vec::new();

    println!("Loading workspace members...");
    for member_path in &config.workspace.members {
        let path = Path::new(member_path);
        let editor = CrateEditor::new(path)
            .with_context(|| format!("Failed to load member at {}", member_path))?;
        editors.push(editor);
    }

    // Collect all package names to know which dependencies to update
    let member_names: HashSet<String> = editors
        .iter()
        .filter_map(|e| e.get_package_name())
        .collect();

    println!("Found {} members: {:?}", member_names.len(), member_names);

    for editor in &mut editors {
        let name = editor.get_package_name().unwrap_or_default();
        println!("Updating {}...", name);

        editor.bump_version(new_version)?;

        // Convert HashSet to Vec for the API I designed in editor.rs (oops, I designed it as slice, so strict ref is okay)
        // Actually editor.rs takes &[String]. HashSet doesn't blindly turn into slice.
        // I should update editor.rs or just collect here.
        // Let's collect to a sorted vec for stability or just iterate.
        let member_names_vec: Vec<String> = member_names.iter().cloned().collect();
        editor.update_dependencies(&member_names_vec, new_version)?;

        editor.save()?;
    }

    println!("Successfully bumped all crates to {}", new_version);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_workspace_integration() -> Result<()> {
        let temp_dir = tempdir()?;
        let workspace_root = temp_dir.path();

        // Create workspace structure programmatically
        fs::write(
            workspace_root.join("Meta.toml"),
            r#"[workspace]
members = [
    "crate_a",
    "crate_b",
    "crate_c"
]
"#,
        )?;

        fs::create_dir(workspace_root.join("crate_a"))?;
        fs::write(
            workspace_root.join("crate_a/Cargo.toml"),
            r#"[package]
name = "crate_a"
version = "0.1.0"
edition = "2021"

[dependencies]
"#,
        )?;

        fs::create_dir(workspace_root.join("crate_b"))?;
        fs::write(
            workspace_root.join("crate_b/Cargo.toml"),
            r#"[package]
name = "crate_b"
version = "0.1.0"
edition = "2021"

[dependencies]
crate_a = { git = "https://github.com/foo/crate_a", tag = "v0.1.0" }
start-up = "1.0"
special-package = { path = "../crate_c", version = "0.1.0" }
"#,
        )?;

        fs::create_dir(workspace_root.join("crate_c"))?;
        fs::write(
            workspace_root.join("crate_c/Cargo.toml"),
            r#"[package]
name = "special-package"
version = "0.1.0"
edition = "2021"
"#,
        )?;

        // Change current directory to temp_dir so MetaConfig::load() finds Meta.toml
        // But changing CWD in test is dangerous for parallel tests.
        // Instead, we should probably refactor `MetaConfig::load` to accept a path?
        // Or refactor `bump_all` to take a config?

        // Let's refactor `MetaConfig::load` to take an optional path or just make a private load_from_path

        // Actually, for this test, let's just use `bump_all` logic inline or modify `bump_all`.
        // `bump_all` calls `MetaConfig::load()`.

        // Refactoring `MetaConfig::load` is the cleanest way.
        // But for now, to avoid changing too much code, I can manually verify the steps in the test
        // by loading config manually and calling editors.

        let config_path = workspace_root.join("Meta.toml");
        let content = fs::read_to_string(&config_path)?;
        let config: MetaConfig = toml_edit::de::from_str(&content)?;

        let new_version = Version::parse("0.2.0")?;
        let mut editors = Vec::new();

        for member_path in &config.workspace.members {
            let path = workspace_root.join(member_path);
            let editor = CrateEditor::new(&path)?;
            editors.push(editor);
        }

        let member_names: HashSet<String> = editors
            .iter()
            .filter_map(|e| e.get_package_name())
            .collect();

        for editor in &mut editors {
            editor.bump_version(&new_version)?;
            let member_names_vec: Vec<String> = member_names.iter().cloned().collect();
            editor.update_dependencies(&member_names_vec, &new_version)?;
            editor.save()?;
        }

        // Verify crate_a
        let crate_a_toml = fs::read_to_string(workspace_root.join("crate_a/Cargo.toml"))?;
        assert!(crate_a_toml.contains(r#"version = "0.2.0""#));

        // Verify crate_b
        let crate_b_toml = fs::read_to_string(workspace_root.join("crate_b/Cargo.toml"))?;
        assert!(crate_b_toml.contains(r#"version = "0.2.0""#));
        // Verify dependency update
        assert!(
            crate_b_toml.contains(
                r#"crate_a = { git = "https://github.com/foo/crate_a", tag = "v0.2.0" }"#
            )
        );
        // Verify mismatch package name update
        assert!(
            crate_b_toml
                .contains(r#"special-package = { path = "../crate_c", version = "0.2.0" }"#)
        );

        Ok(())
    }

    #[test]
    fn test_init_command() -> Result<()> {
        let temp_dir = tempdir()?;
        let workspace_root = temp_dir.path();

        // Create dummy crates
        fs::create_dir(workspace_root.join("crate_x"))?;
        fs::write(
            workspace_root.join("crate_x/Cargo.toml"),
            r#"[package]
name = "crate_x"
version = "0.1.0"
"#,
        )?;

        fs::create_dir(workspace_root.join("crate_y"))?;
        fs::write(
            workspace_root.join("crate_y/Cargo.toml"),
            r#"[package]
name = "crate_y"
version = "0.1.0"
"#,
        )?;

        // We can't really call `generate_meta` directly because it relies on `std::env::current_dir()`.
        // To test it, we either need to change CWD (unsafe in multithreaded tests)
        // or refactor `generate_meta` to take a path.
        // Given I already wrote `generate_meta` to use `current_dir`, I should refactor it slightly to perform the core logic on a path.
        // But for time being, I can't easily change CWD.
        // Let's refactor `generate_meta` to `generate_meta_at(path: &Path)`.

        generate_meta_at(workspace_root)?;

        let meta_toml_path = workspace_root.join("Meta.toml");
        assert!(meta_toml_path.exists());

        let content = fs::read_to_string(meta_toml_path)?;
        assert!(content.contains(r#""crate_x""#));
        assert!(content.contains(r#""crate_y""#));

        Ok(())
    }
    #[test]
    #[ignore]
    fn generate_manual_workspace() -> Result<()> {
        let root = std::env::current_dir()?.join("tests_workspace");
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir(&root)?;

        fs::write(
            root.join("Meta.toml"),
            r#"[workspace]
members = [
    "crate_a",
    "crate_b",
    "crate_c"
]
"#,
        )?;

        fs::create_dir(root.join("crate_a"))?;
        fs::write(
            root.join("crate_a/Cargo.toml"),
            r#"[package]
name = "crate_a"
version = "0.1.0"
edition = "2021"

[dependencies]
"#,
        )?;

        fs::create_dir(root.join("crate_b"))?;
        fs::write(
            root.join("crate_b/Cargo.toml"),
            r#"[package]
name = "crate_b"
version = "0.1.0"
edition = "2021"

[dependencies]
crate_a = { git = "https://github.com/foo/crate_a", tag = "v0.1.0" }
start-up = "1.0"
special-package = { path = "../crate_c", version = "0.1.0" }
"#,
        )?;

        fs::create_dir(root.join("crate_c"))?;
        fs::write(
            root.join("crate_c/Cargo.toml"),
            r#"[package]
name = "special-package"
version = "0.1.0"
edition = "2021"
"#,
        )?;

        println!("Created tests_workspace at {}", root.display());
        Ok(())
    }

    #[test]
    fn test_git_integration() -> Result<()> {
        // 1. Setup temp workspace with git repo
        let temp_dir = tempdir()?;
        let root = temp_dir.path();

        // Init git repo
        let status = std::process::Command::new("git")
            .current_dir(root)
            .args(&["init"])
            .status()?;
        assert!(status.success());

        // Configure minimal git user for commit to work
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["config", "user.email", "you@example.com"])
            .status()?;
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["config", "user.name", "Your Name"])
            .status()?;

        // Create Cargo.toml to define version
        fs::write(
            root.join("Cargo.toml"),
            r#"[package]
name = "test-pkg"
version = "1.2.3"
"#,
        )?;

        // Create initial commit
        fs::write(root.join("README.md"), "init")?;
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["add", "."])
            .status()?;
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["commit", "-m", "Initial"])
            .status()?;

        // Verify git::create_branch works on this repo directly
        crate::git::create_branch(root, "feature-x")?;

        let output = std::process::Command::new("git")
            .current_dir(root)
            .args(&["branch"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("feature-x"));

        // Test Tag
        crate::git::create_tag(root)?;
        let output = std::process::Command::new("git")
            .current_dir(root)
            .args(&["tag"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("v1.2.3"));

        // Setup mock remote for PushTag test
        let remote_dir = temp_dir.path().join("remote.git");
        std::process::Command::new("git")
            .args(&["init", "--bare", remote_dir.to_str().unwrap()])
            .status()?;
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["remote", "add", "origin", remote_dir.to_str().unwrap()])
            .status()?;

        // Test PushTag
        crate::git::push_tag(root)?;

        // Verify tag exists in remote
        let output = std::process::Command::new("git")
            .current_dir(&remote_dir)
            .args(&["tag"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains("v1.2.3"));

        Ok(())
    }

    #[test]
    fn test_commit_specifics() -> Result<()> {
        // Setup repo
        let temp_dir = tempdir()?;
        let root = temp_dir.path();

        let status = std::process::Command::new("git")
            .current_dir(root)
            .args(&["init"])
            .status()?;
        assert!(status.success());

        std::process::Command::new("git")
            .current_dir(root)
            .args(&["config", "user.email", "you@example.com"])
            .status()?;
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["config", "user.name", "Your Name"])
            .status()?;

        // Create Cargo.toml and another file
        let cargo_path = root.join("Cargo.toml");
        fs::write(&cargo_path, "[package]\nname=\"foo\"\nversion=\"0.1.0\"")?;

        let random_path = root.join("random.txt");
        fs::write(&random_path, "initial content")?;

        // Initial commit
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["add", "."])
            .status()?;
        std::process::Command::new("git")
            .current_dir(root)
            .args(&["commit", "-m", "Initial"])
            .status()?;

        // Modify both
        fs::write(&cargo_path, "[package]\nname=\"foo\"\nversion=\"0.2.0\"")?;
        fs::write(&random_path, "modified content")?;

        // Run git::commit via our new logic
        crate::git::commit(root, "update cargo", &[cargo_path.clone()])?;

        // Verify status: valid commit, random.txt modified but not staged
        let output = std::process::Command::new("git")
            .current_dir(root)
            .args(&["status", "--porcelain"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?;

        // M random.txt (modified in work tree)
        // clean cargo.toml (already committed, so not in status or at least not modified relative to index if staged and committed)
        // If committed, it should show as clean.
        // Wait, 'M' in porcelain means modified. If we committed, it should be clean.
        assert!(stdout.contains("M random.txt"));
        assert!(!stdout.contains("M Cargo.toml"));

        // Verify log
        let output = std::process::Command::new("git")
            .current_dir(root)
            .args(&["log", "-1", "--pretty=%B"])
            .output()?;
        let stdout = String::from_utf8(output.stdout)?.trim().to_string();
        assert_eq!(stdout, "update cargo");

        Ok(())
    }
}
