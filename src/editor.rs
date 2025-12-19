use anyhow::{Context, Result};
use semver::Version;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Value, value};

pub struct CrateEditor {
    path: PathBuf,
    doc: DocumentMut,
}

impl CrateEditor {
    pub fn new(path: &Path) -> Result<Self> {
        let manifest_path = path.join("Cargo.toml");
        let content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read Cargo.toml at {:?}", manifest_path))?;
        let doc = content
            .parse::<DocumentMut>()
            .with_context(|| format!("Failed to parse Cargo.toml at {:?}", manifest_path))?;

        Ok(Self {
            path: path.to_path_buf(),
            doc,
        })
    }

    pub fn bump_version(&mut self, new_version: &Version) -> Result<()> {
        self.doc["package"]["version"] = value(new_version.to_string());
        Ok(())
    }

    pub fn update_dependencies(&mut self, members: &[String], new_version: &Version) -> Result<()> {
        // Iterate over table types that contain dependencies
        let tables = ["dependencies", "dev-dependencies", "build-dependencies"];

        for table_name in tables {
            if let Some(table) = self.doc.get_mut(table_name).and_then(|t| t.as_table_mut()) {
                for (dep_name, dep_item) in table.iter_mut() {
                    // Check if the dependency is one of our workspace members
                    // We need to implement a way to map member paths to package names potentially,
                    // or for now assume dependency name matches package name or directory name?
                    // Implementation plan assumption: "Identify dependencies that match other packages in the workspace"
                    // Correct approach: We need to know the PACKAGE NAME of each member.
                    // But for this pass, we might need that mapping passed in.
                    // For now, let's assume we have a set of member names.
                    if members.contains(&dep_name.to_string()) {
                        if let Some(item) = dep_item.as_inline_table_mut() {
                            if item.contains_key("version") {
                                item.insert("version", Value::from(new_version.to_string()));
                            }

                            // Check for branch and replace with tag
                            if item.contains_key("branch") {
                                item.remove("branch");
                                item.insert("tag", Value::from(format!("v{}", new_version)));
                            } else if let Some(tag_item) = item.get_mut("tag") {
                                if let Some(tag_str) = tag_item.as_str() {
                                    let has_v = tag_str.starts_with('v');
                                    let new_tag = if has_v {
                                        format!("v{}", new_version)
                                    } else {
                                        new_version.to_string()
                                    };
                                    *tag_item = Value::from(new_tag);
                                }
                            }
                        } else if dep_item.is_value() {
                            // Handle simple "dep = '1.0'"
                            *dep_item = value(new_version.to_string());
                        }
                        // TODO: Handle struct-like tables? e.g. [dependencies.foo]
                    }
                }
            }
        }
        Ok(())
    }

    pub fn get_package_name(&self) -> Option<String> {
        self.doc
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .map(|s| s.to_string())
    }

    pub fn get_version(&self) -> Option<Version> {
        self.doc
            .get("package")
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .and_then(|s| Version::parse(s).ok())
    }

    pub fn save(&self) -> Result<()> {
        let manifest_path = self.path.join("Cargo.toml");
        fs::write(manifest_path, self.doc.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bump_version() -> Result<()> {
        // Create a temp dir
        let temp_dir = tempfile::tempdir()?;
        let manifest_path = temp_dir.path().join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"[package]
name = "test-crate"
version = "0.1.0"

[dependencies]
other-crate = { version = "0.1.0" }
"#,
        )?;

        let mut editor = CrateEditor::new(temp_dir.path())?;

        // Bump version
        let new_version = Version::parse("0.2.0")?;
        editor.bump_version(&new_version)?;

        // Save
        editor.save()?;

        // Verify
        let content = fs::read_to_string(manifest_path)?;
        assert!(content.contains(r#"version = "0.2.0""#));

        Ok(())
    }

    #[test]
    fn test_update_dependencies() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let manifest_path = temp_dir.path().join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
dep-a = { version = "0.1.0" }
dep-b = "0.1.0"
external-dep = "1.0.0"
"#,
        )?;

        let mut editor = CrateEditor::new(temp_dir.path())?;
        let new_version = Version::parse("0.2.0")?;

        let members = vec!["dep-a".to_string(), "dep-b".to_string()];
        editor.update_dependencies(&members, &new_version)?;
        editor.save()?;

        let content = fs::read_to_string(manifest_path)?;
        assert!(content.contains(r#"dep-a = { version = "0.2.0" }"#));
        assert!(content.contains(r#"dep-b = "0.2.0""#));
        assert!(content.contains(r#"external-dep = "1.0.0""#)); // Should not change

        Ok(())
    }
    #[test]
    fn test_update_git_dependencies() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let manifest_path = temp_dir.path().join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
git-dep-v = { git = "https://example.com/repo", tag = "v0.1.0" }
git-dep-no-v = { git = "https://example.com/repo2", tag = "0.1.0" }
"#,
        )?;

        let mut editor = CrateEditor::new(temp_dir.path())?;
        let new_version = Version::parse("0.2.0")?;

        let members = vec!["git-dep-v".to_string(), "git-dep-no-v".to_string()];

        editor.update_dependencies(&members, &new_version)?;
        editor.save()?;

        let content = fs::read_to_string(manifest_path)?;
        assert!(content.contains(r#"tag = "v0.2.0""#));
        assert!(content.contains(r#"tag = "0.2.0""#));

        Ok(())
    }
    #[test]
    fn test_update_git_branch_to_tag() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let manifest_path = temp_dir.path().join("Cargo.toml");
        fs::write(
            &manifest_path,
            r#"[package]
name = "my-crate"
version = "0.1.0"

[dependencies]
git-dep = { git = "https://example.com/repo", branch = "master" }
"#,
        )?;

        let mut editor = CrateEditor::new(temp_dir.path())?;
        let new_version = Version::parse("0.2.0")?;

        let members = vec!["git-dep".to_string()];

        editor.update_dependencies(&members, &new_version)?;
        editor.save()?;

        let content = fs::read_to_string(manifest_path)?;
        assert!(!content.contains("branch"));
        assert!(content.contains(r#"tag = "v0.2.0""#));

        Ok(())
    }
}
