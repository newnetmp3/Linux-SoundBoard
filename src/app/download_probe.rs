use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::download_validation::validate_download_as;

const PROBE_BYTES: &str = "0-65535";
const PROBE_TIMEOUT_SECS: &str = "30";
const USER_AGENT: &str =
    concat!("Linux-SoundBoard/", env!("CARGO_PKG_VERSION"), " source preflight");
static PROBE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone)]
pub struct DownloadProbe {
    pub content_type: String,
    pub effective_url: String,
}

pub fn probe_download_start(
    provider: &'static str,
    url: &str,
    expected_extension: &str,
    authorization: Option<&str>,
) -> Result<DownloadProbe, String> {
    if which::which("curl").is_err() {
        return Err("curl is not installed".to_string());
    }

    let number = PROBE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "linux-soundboard-probe-{}-{number}.{expected_extension}",
        std::process::id()
    ));
    let _guard = ProbeFileGuard(path.clone());

    let mut command = Command::new("curl");
    command.args([
        "--fail",
        "--location",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "10",
        "--max-time",
        PROBE_TIMEOUT_SECS,
        "--retry",
        "1",
        "--retry-delay",
        "1",
        "--user-agent",
        USER_AGENT,
        "--range",
        PROBE_BYTES,
        "--header",
        "Accept-Encoding: identity",
        "--write-out",
        "%{content_type}\n%{http_code}\n%{url_effective}",
    ]);
    if let Some(header) = authorization {
        command.args(["--header", header]);
    }
    command.arg("--output").arg(&path).arg(url);

    let output = command.output().map_err(|error| {
        format!("{provider} preflight could not start curl for {url}: {error}")
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("{provider} preflight request failed for {url}")
        } else {
            format!("{provider} preflight request failed: {stderr}")
        });
    }

    let write_out = String::from_utf8_lossy(&output.stdout);
    let mut lines = write_out.lines();
    let content_type = lines.next().unwrap_or_default().trim().to_string();
    let status = lines.next().unwrap_or_default().trim().to_string();
    let effective_url = lines.next().unwrap_or(url).trim().to_string();

    if content_type.to_ascii_lowercase().contains("text/html")
        || content_type.to_ascii_lowercase().contains("application/json")
    {
        return Err(format!(
            "{provider} resolved to {content_type} instead of .{expected_extension} data ({effective_url})"
        ));
    }

    validate_download_as(&path, &PathBuf::from(format!("probe.{expected_extension}")))
        .map_err(|message| {
            format!(
                "{provider} returned HTTP {status} / {content_type}, but the payload is not valid .{expected_extension}: {message}"
            )
        })?;

    Ok(DownloadProbe {
        content_type,
        effective_url,
    })
}

struct ProbeFileGuard(PathBuf);

impl Drop for ProbeFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_range_is_bounded() {
        assert_eq!(PROBE_BYTES, "0-65535");
    }
}
