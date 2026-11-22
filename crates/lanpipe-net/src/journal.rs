//! 断点续传的接收端状态记录（journal）。
//!
//! 每个「接收根目录 + manifest 摘要」对应一个 JSONL 文件，逐行记录已完成的文件：
//! `{"id":1,"size":123,"xxh64":"..."}`。追加写 + flush（不逐条 fsync），
//! 崩溃最多丢最近若干条记录，恢复时按 size 复核或重传。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lanpipe_proto::ManifestEntry;
use xxhash_rust::xxh64::xxh64;

/// 对清单做稳定摘要：按 path 排序后对 `path\0size\0mtime\0is_dir\n` 做 xxHash64。
pub fn manifest_digest(entries: &[ManifestEntry]) -> String {
    let mut sorted: Vec<&ManifestEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut buf = Vec::with_capacity(entries.len() * 48);
    for e in sorted {
        buf.extend_from_slice(e.path.as_bytes());
        buf.push(0);
        buf.extend_from_slice(&e.size.to_be_bytes());
        buf.push(0);
        buf.extend_from_slice(&e.mtime_ms.to_be_bytes());
        buf.push(0);
        buf.push(u8::from(e.is_dir));
        buf.push(b'\n');
    }
    format!("{:016x}", xxh64(&buf, 0))
}

/// 接收端状态目录（跨平台，可用 `LANPIPE_STATE_DIR` 覆盖）。
pub fn default_state_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LANPIPE_STATE_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("LanPipe").join("state");
        }
    } else {
        if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
            return PathBuf::from(xdg).join("lanpipe");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".local").join("state").join("lanpipe");
        }
    }
    std::env::temp_dir().join("lanpipe-state")
}

/// journal 默认保留时长：30 天。过期条目对应的是很久以前的中断任务，继续复用的价值很低。
pub const JOURNAL_TTL_SECS: u64 = 30 * 24 * 60 * 60;

/// 清理 `state_dir/journals` 下修改时间早于 `now - max_age_secs` 的 journal，返回删除数量。
pub fn prune(state_dir: &Path, max_age_secs: u64) -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    prune_before(state_dir, now.saturating_sub(max_age_secs))
}

/// 删除修改时间早于 `cutoff_secs`（Unix 秒）的 journal 文件；`cutoff_secs = 0` 时全部保留。
pub fn prune_before(state_dir: &Path, cutoff_secs: u64) -> usize {
    let dir = state_dir.join("journals");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(since) = modified.duration_since(UNIX_EPOCH) else {
            continue;
        };
        if since.as_secs() < cutoff_secs && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[derive(Debug, Clone, Default)]
pub struct JournalData {
    /// 首次会话使用的接收根目录名（续传时复用，避免落到新的 `LanPipe-<时间戳>` 根）。
    pub root_name: Option<String>,
    pub records: HashMap<u32, JournalRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalRecord {
    pub id: u32,
    pub size: u64,
    pub xxh64: u64,
}

fn journal_path(root: &Path, dest: &Path, digest: &str) -> PathBuf {
    let dest_key = format!("{:016x}", xxh64(dest.to_string_lossy().as_bytes(), 0));
    root.join("journals").join(format!("{dest_key}-{digest}.jsonl"))
}

/// 读取某个会话的 journal（根目录名 + 已完成文件）；坏行忽略。
pub fn load(root: &Path, dest: &Path, digest: &str) -> JournalData {
    let path = journal_path(root, dest, digest);
    let mut out = JournalData::default();
    let Ok(file) = std::fs::File::open(&path) else {
        return out;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(name) = v.get("root").and_then(|x| x.as_str()) {
            out.root_name = Some(name.to_string());
            continue;
        }
        let (Some(id), Some(size), Some(hash)) = (
            v.get("id").and_then(|x| x.as_u64()),
            v.get("size").and_then(|x| x.as_u64()),
            v.get("xxh64").and_then(|x| x.as_u64()),
        ) else {
            continue;
        };
        out.records.insert(
            id as u32,
            JournalRecord {
                id: id as u32,
                size,
                xxh64: hash,
            },
        );
    }
    out
}

/// 追加写句柄：每个会话一个。
pub struct JournalWriter {
    path: PathBuf,
    file: std::sync::Mutex<std::fs::File>,
}

impl JournalWriter {
    pub fn open(
        root: &Path,
        dest: &Path,
        digest: &str,
        root_name: &str,
    ) -> Result<Self, String> {
        let path = journal_path(root, dest, digest);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("创建状态目录失败: {e}"))?;
        }
        let is_new = std::fs::metadata(&path).map(|m| m.len() == 0).unwrap_or(true);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("打开 journal 失败: {e}"))?;
        if is_new {
            let header = format!("{{\"root\":{}}}\n", serde_json::json!(root_name));
            let _ = file.write_all(header.as_bytes());
            let _ = file.flush();
        }
        Ok(Self {
            path,
            file: std::sync::Mutex::new(file),
        })
    }

    pub fn record(&self, id: u32, size: u64, hash: u64) {
        let line = format!("{{\"id\":{id},\"size\":{size},\"xxh64\":{hash}}}\n");
        let mut guard = self
            .file
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _ = guard.write_all(line.as_bytes());
        let _ = guard.flush();
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, size: u64, mtime: u64) -> ManifestEntry {
        ManifestEntry {
            id: 1,
            path: path.to_string(),
            size,
            mtime_ms: mtime,
            is_dir: false,
        }
    }

    #[test]
    fn digest_is_order_independent() {
        let a = entry("b.txt", 3, 10);
        let b = entry("a.txt", 1, 20);
        let d1 = manifest_digest(&[a.clone(), b.clone()]);
        let d2 = manifest_digest(&[b, a]);
        assert_eq!(d1, d2);
    }

    #[test]
    fn journal_roundtrip_and_tolerates_bad_lines() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("lanpipe-journal-{}", std::process::id()));
        let dest = root.join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        let digest = "deadbeefdeadbeef";
        let writer = JournalWriter::open(&root, &dest, digest, "LanPipe-test").unwrap();
        writer.record(7, 123, 999);
        std::fs::write(
            writer.path(),
            format!(
                "{}\nnot-json\n",
                std::fs::read_to_string(writer.path()).unwrap().trim()
            ),
        )
        .ok();
        // 重新追加一行合法记录（模拟后续写入）
        std::fs::OpenOptions::new()
            .append(true)
            .open(writer.path())
            .unwrap()
            .write_all(b"{\"id\":8,\"size\":5,\"xxh64\":1}\n")
            .unwrap();
        let loaded = load(&root, &dest, digest);
        assert_eq!(loaded.root_name.as_deref(), Some("LanPipe-test"));
        assert_eq!(loaded.records.get(&7).map(|r| r.size), Some(123));
        assert_eq!(loaded.records.get(&8).map(|r| r.xxh64), Some(1));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_removes_only_expired_journals() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(format!("lanpipe-prune-{}", std::process::id()));
        let journals = root.join("journals");
        std::fs::create_dir_all(&journals).unwrap();
        let keep = journals.join("keep-1.jsonl");
        let drop = journals.join("drop-1.jsonl");
        std::fs::write(&keep, b"{}\n").unwrap();
        std::fs::write(&drop, b"{}\n").unwrap();

        // cutoff=0：什么都不删
        assert_eq!(prune_before(&root, 0), 0);
        assert!(keep.exists() && drop.exists());

        // cutoff 设为未来：两个都算过期
        let future = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;
        assert_eq!(prune_before(&root, future), 2);
        assert!(!keep.exists() && !drop.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prune_ignores_missing_directory() {
        let missing = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join("lanpipe-prune-missing");
        let _ = std::fs::remove_dir_all(&missing);
        assert_eq!(prune(&missing, JOURNAL_TTL_SECS), 0);
    }
}
