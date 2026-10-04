use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static DIAGNOSTIC_PATH: OnceLock<PathBuf> = OnceLock::new();
static DIAGNOSTIC_LOCK: Mutex<()> = Mutex::new(());

fn append_diagnostic(path: &Path, record: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(record.as_bytes())?;
    file.flush()?;
    file.sync_data()
}

fn diagnostic_record(kind: &str, detail: &str) -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{{\"epoch_seconds\":{},\"epoch_nanos\":{},\"kind\":{},\"detail\":{}}}\n",
        epoch.as_secs(),
        epoch.subsec_nanos(),
        serde_json::to_string(kind).unwrap_or_else(|_| "\"stage\"".into()),
        serde_json::to_string(detail).unwrap_or_else(|_| "\"unavailable\"".into())
    )
}

fn write_stage(path: &Path, stage: &'static str) {
    let _guard = DIAGNOSTIC_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let record = diagnostic_record("stage", stage);
    let _ = append_diagnostic(path, &record);
}

fn panic_record(location: &str, backtrace: &str) -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{{\"epoch_seconds\":{},\"epoch_nanos\":{},\"kind\":\"panic\",\"location\":{},\"backtrace\":{}}}\n",
        epoch.as_secs(),
        epoch.subsec_nanos(),
        serde_json::to_string(location).unwrap_or_else(|_| "\"unknown\"".into()),
        serde_json::to_string(backtrace).unwrap_or_else(|_| "\"unavailable\"".into())
    )
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    #[test]
    fn diagnostic_records_are_durable_and_exclude_panic_payloads() {
        let path = std::env::temp_dir().join(format!(
            "pomme-diagnostic-{}-{}.jsonl",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        write_stage(&path, "test.stage");
        write_image(&path, "test.image", 3, 4, 2, 1);
        let stage_file = std::fs::read_to_string(&path).unwrap();
        assert!(stage_file.contains("test.stage"));
        assert!(stage_file.contains("\"width\":3,\"height\":4,\"layers\":2,\"bytes\":24"));

        let private_payload = "private-panic-payload";
        let record = panic_record("src/main.rs:7:3", "captured backtrace");
        assert!(!record.contains(private_payload));
        append_diagnostic(&path, &record).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("src/main.rs:7:3"));
        assert!(contents.contains("captured backtrace"));
        assert!(!contents.contains("private-panic-payload"));
        let _ = std::fs::remove_file(path);
    }
}

fn install_panic_diagnostic(log_dir: &Path) {
    let path = log_dir.join("diagnostic.jsonl");
    let _ = DIAGNOSTIC_PATH.set(path);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(path) = DIAGNOSTIC_PATH.get() {
            // Do not let diagnostic failures or a reentrant hook mask the original panic.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let location = info
                    .location()
                    .map(|loc| {
                        let file = Path::new(loc.file())
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("unknown");
                        format!("{file}:{}:{}", loc.line(), loc.column())
                    })
                    .unwrap_or_else(|| "unknown".to_owned());
                let backtrace = std::backtrace::Backtrace::force_capture().to_string();
                let backtrace = ["USERPROFILE", "HOME"]
                    .into_iter()
                    .filter_map(|key| std::env::var(key).ok())
                    .filter(|home| !home.is_empty())
                    .fold(backtrace, |text, home| text.replace(&home, "<home>"));
                let record = panic_record(&location, &backtrace);
                let _guard = match DIAGNOSTIC_LOCK.try_lock() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                let _ = append_diagnostic(path, &record);
            }));
        }
        previous(info);
    }));
}

pub(crate) fn diagnostic_stage(stage: &'static str) {
    if let Some(path) = DIAGNOSTIC_PATH.get() {
        write_stage(path, stage);
    }
}

fn write_image(
    path: &Path,
    stage: &'static str,
    width: u32,
    height: u32,
    layers: u32,
    bytes_per_pixel: u32,
) {
    let _guard = DIAGNOSTIC_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let byte_count = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|bytes| bytes.checked_mul(u64::from(layers)))
        .and_then(|bytes| bytes.checked_mul(u64::from(bytes_per_pixel)))
        .unwrap_or(u64::MAX);
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let record = format!(
        "{{\"epoch_seconds\":{},\"epoch_nanos\":{},\"kind\":\"image\",\"stage\":{},\"width\":{},\"height\":{},\"layers\":{},\"bytes\":{}}}\n",
        epoch.as_secs(), epoch.subsec_nanos(),
        serde_json::to_string(stage).unwrap_or_else(|_| "\"stage\"".into()),
        width, height, layers, byte_count
    );
    let _ = append_diagnostic(path, &record);
}

pub(crate) fn diagnostic_image(
    stage: &'static str,
    width: u32,
    height: u32,
    layers: u32,
    bytes_per_pixel: u32,
) {
    if let Some(path) = DIAGNOSTIC_PATH.get() {
        write_image(path, stage, width, height, layers, bytes_per_pixel);
    }
}

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init(log_dir: &Path) -> WorkerGuard {
    install_panic_diagnostic(log_dir);
    let file_appender = tracing_appender::rolling::never(log_dir, "latest.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_filter = EnvFilter::new("debug");
    let stdout_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking)
                .with_filter(file_filter),
        )
        .with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_filter(stdout_filter),
        )
        .init();

    guard
}

const MAX_LOG_FILES: usize = 5;

pub fn rotate(log_dir: &Path) -> std::io::Result<()> {
    let latest = log_dir.join("latest.log");
    if !latest.exists() {
        return Ok(());
    }
    let modified = latest.metadata()?.modified()?;

    let datetime = time::OffsetDateTime::from(modified);
    let date = datetime
        .format(time::macros::format_description!("[year]-[month]-[day]"))
        .map_err(std::io::Error::other)?;

    let index = (1..)
        .find(|i| !log_dir.join(format!("{date}-{i}.log.gz")).exists())
        .unwrap();
    let dest = log_dir.join(format!("{date}-{index}.log.gz"));

    let input = std::fs::read(&latest)?;
    let output_file = std::fs::File::create(&dest)?;
    let mut encoder = flate2::write::GzEncoder::new(output_file, flate2::Compression::default());

    std::io::Write::write_all(&mut encoder, &input)?;
    encoder.finish().map_err(std::io::Error::other)?;
    std::fs::remove_file(&latest)?;

    cleanup_old_logs(log_dir);

    Ok(())
}

fn cleanup_old_logs(log_dir: &Path) {
    let mut gz_files: Vec<_> = std::fs::read_dir(log_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "gz"))
        .collect();

    if gz_files.len() <= MAX_LOG_FILES {
        return;
    }

    gz_files.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    for entry in &gz_files[..gz_files.len() - MAX_LOG_FILES] {
        let _ = std::fs::remove_file(entry.path());
    }
}
