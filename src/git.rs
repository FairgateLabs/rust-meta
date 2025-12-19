use crate::editor::CrateEditor;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn group_members_by_repo(members: &[PathBuf]) -> Result<HashMap<PathBuf, Vec<PathBuf>>> {
    let mut repo_map: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for member in members {
        if let Some(git_root) = find_git_root(member)? {
            repo_map.entry(git_root).or_default().push(member.clone());
        } else {
            println!("Warning: No git repository found for member {:?}", member);
        }
    }

    Ok(repo_map)
}

fn find_git_root(path: &Path) -> Result<Option<PathBuf>> {
    let mut current = path.canonicalize().context("Failed to canonicalize path")?;

    loop {
        if current.join(".git").exists() {
            return Ok(Some(current));
        }

        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            return Ok(None);
        }
    }
}

pub fn create_branch(repo_path: &Path, name: &str) -> Result<()> {
    println!("Creating/Switching to branch '{}' in {:?}", name, repo_path);
    // try checkout first
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(&["checkout", name])
        .output()?;

    if !status.status.success() {
        // Create new branch
        run_git_cmd(repo_path, &["checkout", "-b", name])?;
    }
    Ok(())
}

pub fn checkout_branch(repo_path: &Path, name: &str) -> Result<()> {
    println!("Checking out '{}' in {:?}", name, repo_path);
    run_git_cmd(repo_path, &["checkout", name])
}

pub fn merge_branch(repo_path: &Path, branch: &str) -> Result<()> {
    println!("Merging '{}' in {:?}", branch, repo_path);
    run_git_cmd(repo_path, &["merge", branch])
}

pub fn remove_branch(repo_path: &Path, name: &str, remote: bool) -> Result<()> {
    println!("Removing branch '{}' in {:?}", name, repo_path);
    // Local delete
    let _ = run_git_cmd(repo_path, &["branch", "-D", name]); // Ignore error if not exists locally or currently checked out?

    if remote {
        println!("Removing remote branch '{}'...", name);
        // Assuming 'origin' is the remote
        run_git_cmd(repo_path, &["push", "origin", "--delete", name])?;
    }
    Ok(())
}

pub fn push(repo_path: &Path) -> Result<()> {
    println!("Pushing in {:?}", repo_path);
    // Get current branch name
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .output()?;
    let branch = String::from_utf8(output.stdout)?.trim().to_string();

    // Push setting upstream
    run_git_cmd(repo_path, &["push", "-u", "origin", &branch])
}

pub fn commit(repo_path: &Path, message: &str, files: &[PathBuf]) -> Result<()> {
    println!("Committing in {:?} with message '{}'", repo_path, message);

    if files.is_empty() {
        println!("No files to commit in {:?}", repo_path);
        return Ok(());
    }

    // 1. Add specific files
    // Convert absolute paths to relative paths strictly for git add (though git add accepts absolute if within repo usually, safer to be relative or just pass them)
    // Actually git add works fine with absolute paths usually, but let's try just passing them.
    let mut args = vec!["add"];
    let file_strs: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    args.extend(file_strs.iter().map(|s| s.as_str()));

    run_git_cmd(repo_path, &args)?;

    // 2. Commit
    run_git_cmd(repo_path, &["commit", "-m", message])
}

pub fn create_tag(repo_path: &Path) -> Result<()> {
    println!("Creating tag in {:?}", repo_path);
    // Same version logic as commit used to have, we keep it for tagging
    let cargo_toml = repo_path.join("Cargo.toml");
    let version_str = if cargo_toml.exists() {
        if let Ok(editor) = CrateEditor::new(repo_path) {
            editor
                .get_version()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    if version_str == "unknown" {
        println!(
            "Skipping tag in {:?}: Could not determine version",
            repo_path
        );
        return Ok(());
    }

    let tag_name = format!("v{}", version_str);
    run_git_cmd(repo_path, &["tag", &tag_name])
}

pub fn push_tag(repo_path: &Path) -> Result<()> {
    println!("Pushing tag in {:?}", repo_path);
    // Same version logic as commit/tag
    let cargo_toml = repo_path.join("Cargo.toml");
    let version_str = if cargo_toml.exists() {
        if let Ok(editor) = CrateEditor::new(repo_path) {
            editor
                .get_version()
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    };

    if version_str == "unknown" {
        println!(
            "Skipping push_tag in {:?}: Could not determine version",
            repo_path
        );
        return Ok(());
    }

    let tag_name = format!("v{}", version_str);
    run_git_cmd(repo_path, &["push", "origin", &tag_name])
}

pub fn remove_tag(repo_path: &Path, name: &str, remote: bool) -> Result<()> {
    println!("Removing tag '{}' in {:?}", name, repo_path);
    let _ = run_git_cmd(repo_path, &["tag", "-d", name]);

    if remote {
        run_git_cmd(repo_path, &["push", "origin", "--delete", name])?;
    }
    Ok(())
}

fn run_git_cmd(repo_path: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .status()
        .context(format!("Failed to execute git {:?}", args))?;

    if !status.success() {
        anyhow::bail!("Git command failed: {:?}", args);
    }
    Ok(())
}
