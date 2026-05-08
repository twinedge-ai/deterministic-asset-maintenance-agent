use crate::schemas::ObservationSnapshot;
use serde::Serialize;
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("failed to open {path}: {source}")]
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse JSON line {line_number} from {path}: {source}")]
    Json {
        path: PathBuf,
        line_number: usize,
        source: serde_json::Error,
    },
    #[error("trace is empty: {path}")]
    Empty { path: PathBuf },
    #[error("as-of selector did not match any snapshot: {selector}")]
    AsOfNotFound { selector: String },
}

pub fn read_trace(path: impl AsRef<Path>) -> Result<Vec<ObservationSnapshot>, ReplayError> {
    let path = path.as_ref().to_path_buf();
    let file = File::open(&path).map_err(|source| ReplayError::Open {
        path: path.clone(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut snapshots = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(|source| ReplayError::Read {
            path: path.clone(),
            source,
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let snapshot = serde_json::from_str(trimmed).map_err(|source| ReplayError::Json {
            path: path.clone(),
            line_number: index + 1,
            source,
        })?;
        snapshots.push(snapshot);
    }
    if snapshots.is_empty() {
        return Err(ReplayError::Empty { path });
    }
    Ok(snapshots)
}

pub fn select_as_of<'a>(
    snapshots: &'a [ObservationSnapshot],
    selector: &str,
) -> Result<&'a ObservationSnapshot, ReplayError> {
    if selector == "trace:last" {
        return snapshots.last().ok_or_else(|| ReplayError::AsOfNotFound {
            selector: selector.to_string(),
        });
    }
    if selector == "trace:first" {
        return snapshots.first().ok_or_else(|| ReplayError::AsOfNotFound {
            selector: selector.to_string(),
        });
    }
    snapshots
        .iter()
        .find(|snapshot| snapshot.as_of_timestamp == selector || snapshot.timestamp == selector)
        .ok_or_else(|| ReplayError::AsOfNotFound {
            selector: selector.to_string(),
        })
}

pub fn write_jsonl<T: Serialize>(path: impl AsRef<Path>, records: &[T]) -> Result<(), ReplayError> {
    let path = path.as_ref().to_path_buf();
    let file = File::create(&path).map_err(|source| ReplayError::Open {
        path: path.clone(),
        source,
    })?;
    let mut writer = BufWriter::new(file);
    for record in records {
        serde_json::to_writer(&mut writer, record).map_err(|source| ReplayError::Json {
            path: path.clone(),
            line_number: 0,
            source,
        })?;
        writer
            .write_all(b"\n")
            .map_err(|source| ReplayError::Read {
                path: path.clone(),
                source,
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_trace, select_as_of};
    use std::path::Path;

    #[test]
    fn reads_low_suction_trace() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace should load");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].asset_id, "hp_pump_1");
    }

    #[test]
    fn select_trace_last_is_deterministic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace should load");
        let first = select_as_of(&snapshots, "trace:last").expect("snapshot exists");
        let second = select_as_of(&snapshots, "trace:last").expect("snapshot exists");
        assert_eq!(first.as_of_timestamp, second.as_of_timestamp);
    }
}
