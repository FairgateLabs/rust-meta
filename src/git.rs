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

pub fn pull(repo_path: &Path) -> Result<()> {
    println!("Pulling in {:?}", repo_path);
    // Get current branch name
    let output = Command::new("git")
        .current_dir(repo_path)
        .args(&["rev-parse", "--abbrev-ref", "HEAD"])
        .output()?;
    let branch = String::from_utf8(output.stdout)?.trim().to_string();

    run_git_cmd(repo_path, &["pull", "origin", &branch])
}

pub fn fetch(repo_path: &Path) -> Result<()> {
    println!("Fetching in {:?}", repo_path);
    run_git_cmd(repo_path, &["fetch", "origin"])
}

pub fn commit(repo_path: &Path, message: &str, files: &[PathBuf]) -> Result<()> {
    println!("Committing in {:?} with message '{}'", repo_path, message);

    if files.is_empty() {
        println!("No files to commit in {:?}", repo_path);
        return Ok(());
    }

    // 1. Add specific files
    // Convert paths to be relative to the repo_root (repo_path)
    let mut args = vec!["add"];
    let mut relative_paths = Vec::new();

    for file in files {
        // We canonicalize to ensure we have an absolute path that matches repo_path's canonical nature.
        // If the file doesn't exist (e.g. deleted), canonicalize fails.
        // In the case of version bumping/modification, it should exist.
        // If it doesn't, we might fallback to just using it as is or skipping.
        let abs_file = if file.exists() {
            file.canonicalize().unwrap_or_else(|_| file.to_path_buf())
        } else {
            // If it doesn't exist, we can't easily strip prefix if it's relative and repo is absolute.
            // But let's assume it's absolute or relative to CWD.
            // For now, let's just try to use it as is if canonicalize fails.
            file.to_path_buf()
        };

        match abs_file.strip_prefix(repo_path) {
            Ok(rel) => relative_paths.push(rel.to_string_lossy().to_string()),
            Err(_) => {
                // If we can't strip prefix, maybe it's already relative or outside repo?
                // Just use the path as provided.
                relative_paths.push(file.to_string_lossy().to_string());
            }
        }
    }

    args.extend(relative_paths.iter().map(|s| s.as_str()));

    run_git_cmd(repo_path, &args)?;

    // 2. Commit
    run_git_cmd(repo_path, &["commit", "-m", message])
}

pub fn create_tag(repo_path: &Path, version: &str) -> Result<()> {
    println!("Creating tag 'v{}' in {:?}", version, repo_path);
    let tag_name = format!("v{}", version);
    run_git_cmd(repo_path, &["tag", &tag_name])
}

pub fn push_tag(repo_path: &Path, version: &str) -> Result<()> {
    println!("Pushing tag 'v{}' in {:?}", version, repo_path);
    let tag_name = format!("v{}", version);
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

pub fn execute_command(work_dir: &Path, command: &str) -> Result<()> {
    let status = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .current_dir(work_dir)
            .args(&["/C", command])
            .status()
    } else {
        Command::new("sh")
            .current_dir(work_dir)
            .args(&["-c", command])
            .status()
    }
    .context(format!("Failed to execute command: {}", command))?;

    if !status.success() {
        anyhow::bail!("Command failed with status: {:?}", status);
    }
    Ok(())
}
