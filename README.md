# meta

`meta` is a CLI tool designed to manage a meta-workspace of Rust crates. It simplifies operations across multiple repositories or crates, allowing you to perform version bumps, git operations, and workspace management in bulk.

## Features

- **Version Bumping**: Bump the version of all crates in the meta-workspace and update dependencies automatically.
- **Git Integration**: Perform git operations on all member repositories simultaneously:
  - Branching (`branch`, `checkout`, `remove-branch`)
  - Merging (`merge`)
  - Committing (`commit`)
  - Pushing/Pulling (`push`, `pull`, `fetch`, `push-tag`)
  - Tagging (`tag`, `remove-tag`)
- **Workspace Initialization**: Easily initialize a new meta-workspace configuration (`Meta.toml`) by scanning the current directory.


## Usage

### Initialize a Meta Workspace

Run `meta init` in the root directory containing your Rust crates. This will scan for subdirectories with `Cargo.toml` and generate a `Meta.toml` configuration file.

```bash
meta init
```

Manually edit the `Meta.toml` file to ensure it contains the correct paths to your crates.

### Version Management

Bump the version of all crates in the workspace directly. This updates `Cargo.toml` versions and dependency references.

```bash
meta bump 0.2.0
```

### Git Operations

Run git commands across all repositories defined in `Meta.toml`.

```bash
# Create a new branch
meta branch feature/new-stuff

# Checkout an existing branch
meta checkout develop

# Commit changes with a custom message (optional message)
meta commit -m "feat: update dependencies"
# If message is omitted, defaults to "bump version <version>" if used after bump

# Push or pull changes
meta push
meta pull
meta fetch

# Create tags and push them
meta tag 1.2.3          # Uses specific version (mandatory)
meta push-tag 1.2.3     # Pushes specific version tag (mandatory)
```

## Configuration

The tool uses a `Meta.toml` file to track workspace members.

```toml
[workspace]
members = [
    "crate-a",
    "libs/crate-b",
    "services/crate-c"
]
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
