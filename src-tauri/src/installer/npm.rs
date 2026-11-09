use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use super::mcp_fs::{private_directory, real_file, write_new};

pub(super) const NPM_CLI: &str = "lib/node_modules/npm/bin/npm-cli.js";

pub(super) struct NpmEnvironment {
    root: PathBuf,
    runtime: PathBuf,
    project: PathBuf,
}

impl NpmEnvironment {
    pub(super) fn create(attempt: &Path, runtime: &Path, project: &Path) -> Result<Self, String> {
        let root = attempt.join("npm-environment");
        private_directory(&root)?;
        for name in ["home", "cache", "tmp", "config", "prefix"] {
            private_directory(&root.join(name))?;
        }
        write_new(&root.join("user.npmrc"), b"")?;
        write_new(&root.join("global.npmrc"), b"")?;
        Ok(Self {
            root,
            runtime: runtime.to_path_buf(),
            project: project.to_path_buf(),
        })
    }

    pub(super) fn command(&self, action: &str) -> Result<Command, String> {
        let mut command = Command::new(real_file(&self.runtime, Path::new("bin/node"))?);
        command
            .arg(real_file(&self.runtime, Path::new(NPM_CLI))?)
            .arg(action);
        command
            .current_dir(&self.project)
            .env_clear()
            .env("PATH", self.runtime.join("bin"))
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("TMP", self.root.join("tmp"))
            .env("TEMP", self.root.join("tmp"))
            .env("npm_config_cache", self.root.join("cache"))
            .env("npm_config_userconfig", self.root.join("user.npmrc"))
            .env("npm_config_globalconfig", self.root.join("global.npmrc"))
            .env("npm_config_prefix", self.root.join("prefix"))
            .args([
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
                "--no-bin-links",
                "--no-progress",
                "--loglevel=silent",
                "--logs-max=0",
                "--update-notifier=false",
                "--global=false",
                "--workspaces=false",
                "--package-lock=true",
                "--registry=https://registry.npmjs.org/",
            ]);
        Ok(command)
    }

    pub(super) async fn ci(&self) -> Result<(), String> {
        let mut command = self.command("ci")?;
        command.arg("--strict-peer-deps");
        super::process::run(&mut command, Duration::from_secs(600), false)
            .await
            .map(|_| ())
            .map_err(|error| format!("npm ci: {error}"))
    }

    pub(super) async fn validate_dependencies(&self) -> Result<(), String> {
        let mut command = self.command("ls")?;
        command.args(["--all", "--offline", "--json"]);
        super::process::run(&mut command, Duration::from_secs(30), false)
            .await
            .map(|_| ())
            .map_err(|error| format!("npm dependency validation: {error}"))
    }
}
