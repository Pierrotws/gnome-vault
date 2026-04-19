use std::{
    path::Path,
    process::{Command, ExitStatus},
};

#[derive(Debug)]
pub enum GitError {
    Io(std::io::Error),
    Utf8(std::string::FromUtf8Error),
    CommandFailed {
        command: String,
        status: ExitStatus,
        stderr: String,
    },
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::Io(err) => write!(f, "I/O error: {err}"),
            GitError::Utf8(err) => write!(f, "UTF-8 error: {err}"),
            GitError::CommandFailed {
                command,
                status,
                stderr,
            } => {
                if stderr.trim().is_empty() {
                    write!(f, "{command} failed with status {status}")
                } else {
                    write!(
                        f,
                        "{command} failed with status {status}: {}",
                        stderr.trim()
                    )
                }
            }
        }
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(value: std::io::Error) -> Self {
        GitError::Io(value)
    }
}

impl From<std::string::FromUtf8Error> for GitError {
    fn from(value: std::string::FromUtf8Error) -> Self {
        GitError::Utf8(value)
    }
}

fn run_command(cmd: &mut Command, command: &str) -> Result<(), GitError> {
    let output = cmd.output()?;
    if !output.status.success() {
        return Err(GitError::CommandFailed {
            command: command.to_string(),
            status: output.status,
            stderr: String::from_utf8(output.stderr)?,
        });
    }
    Ok(())
}

pub fn add(project_dir: &Path, file_path: &Path) -> Result<(), GitError> {
    run_command(
        Command::new("git")
            .current_dir(project_dir)
            .arg("add")
            .arg(file_path),
        "git add",
    )
}

pub fn commit(project_dir: &Path, message: &str) -> Result<(), GitError> {
    run_command(
        Command::new("git")
            .current_dir(project_dir)
            .arg("commit")
            .arg("-m")
            .arg(message),
        "git commit",
    )
}

pub fn push(project_dir: &Path) -> Result<(), GitError> {
    run_command(
        Command::new("git").current_dir(project_dir).arg("push"),
        "git push",
    )
}
