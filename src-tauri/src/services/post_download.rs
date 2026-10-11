//! Runs the user's program on each finished download, e.g. a virus scanner.

use super::config::RuntimeConfig;
use super::monitor::TaskEvent;

/// Splits a command line at unquoted whitespace and substitutes the
/// placeholders in each argument. Double quotes group, `\"` is a literal quote.
fn arguments(template: &str, values: &[(&str, &str)]) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        args.push(current);
    }
    args.into_iter()
        .map(|arg| {
            values
                .iter()
                .fold(arg, |arg, (name, value)| arg.replace(name, value))
        })
        .collect()
}

/// Starts the configured program for `event` without waiting for it.
pub fn run(config: &RuntimeConfig, event: &TaskEvent) {
    let program = config.post_download_program.trim();
    if program.is_empty() {
        return;
    }
    let path = event.output_path();
    let file = path.to_string_lossy();
    let template = match config.post_download_arguments.trim() {
        "" => "\"{file}\"",
        template => template,
    };
    let args = arguments(
        template,
        &[
            ("{file}", &file),
            ("{dir}", &event.dir),
            ("{name}", &event.name),
        ],
    );
    let mut command = tokio::process::Command::new(program);
    command
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    match command.spawn() {
        Ok(mut child) => {
            log::info!(
                "post-download:started gid={} program={program:?}",
                event.gid
            );
            let gid = event.gid.clone();
            tauri::async_runtime::spawn(async move {
                match child.wait().await {
                    Ok(status) => log::info!("post-download:exited gid={gid} status={status}"),
                    Err(error) => log::warn!("post-download:wait-failed gid={gid} error={error}"),
                }
            });
        }
        Err(error) => log::warn!(
            "post-download:spawn-failed gid={} program={program:?} error={error}",
            event.gid
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::arguments;

    #[test]
    fn arguments_keep_quoted_paths_whole() {
        let values = [
            ("{file}", r"C:\Downloads\my file.exe"),
            ("{dir}", r"C:\Downloads"),
        ];
        assert_eq!(
            arguments(r#"-Scan -File "{file}" --dir={dir}"#, &values),
            [
                "-Scan",
                "-File",
                r"C:\Downloads\my file.exe",
                r"--dir=C:\Downloads"
            ]
        );
        assert_eq!(arguments(r#"  "" say\"hi\" "#, &[]), ["", r#"say"hi""#]);
        assert!(arguments("   ", &[]).is_empty());
    }
}
