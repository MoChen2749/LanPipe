//! 传输引擎：发送端逐文件直写，接收端 `.lp-part → rename`，零暂存归档。
//!
//! 一条控制 TCP 连接承载 Offer/Hello，数据按 `streams` 条连接并行发送；
//! 每个 DATA 帧只携带 `文件 id + 偏移 + 字节`，接收端边收边写目标文件。
//!
//! 并发模型：会话内每个文件一把独立的异步锁，DATA 写入不在全局会话语义上串行；
//! 进度计数用原子量，避免进度上报把并行流压成单流。

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::journal;
use lanpipe_proto::{
    read_frame, write_frame, Accept, ErrorMsg, FileDone, FrameType, Hello, ManifestEntry, Offer,
    Reject, ResumeState, ResyncIds, VerifyDigest, DATA_CHUNK, MAX_CONTROL_FRAME, MAX_DATA_FRAME,
    PROTOCOL_VERSION,
};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time;
use xxhash_rust::xxh64::Xxh64;

/// 传输过程事件，UI 据此刷新设备列表与进度条。
#[derive(Debug, Clone)]
pub enum TransferEvent {
    Incoming {
        session_id: String,
        root_name: String,
        files: u64,
        bytes: u64,
    },
    ReceiveProgress {
        session_id: String,
        files_done: u64,
        files_total: u64,
        bytes_done: u64,
        bytes_total: u64,
    },
    FileReceived {
        session_id: String,
        path: String,
        size: u64,
    },
    Received {
        session_id: String,
        files: u64,
        bytes: u64,
        elapsed_ms: u64,
        root: String,
    },
    SendProgress {
        session_id: String,
        files_done: u64,
        files_total: u64,
        bytes_done: u64,
        bytes_total: u64,
    },
    Sent {
        session_id: String,
        files: u64,
        bytes: u64,
        elapsed_ms: u64,
    },
    Failed {
        session_id: Option<String>,
        message: String,
    },
}

// ---------------------------------------------------------------------------
// 接收端
// ---------------------------------------------------------------------------

type Sessions = Arc<Mutex<HashMap<String, Arc<Session>>>>;

struct PartState {
    file: File,
    written: u64,
    part_path: PathBuf,
    final_path: PathBuf,
    hasher: Xxh64,
}

struct Session {
    id: String,
    root: PathBuf,
    rel_paths: HashMap<u32, String>,
    sizes: HashMap<u32, u64>,
    total_files: u64,
    total_bytes: u64,
    started: Instant,
    events: mpsc::UnboundedSender<TransferEvent>,
    files_done: AtomicU64,
    bytes_done: AtomicU64,
    open: StdMutex<HashMap<u32, Arc<Mutex<PartState>>>>,
    progress_state: StdMutex<(Instant, u64)>,
    received_emitted: std::sync::atomic::AtomicBool,
    /// 已完成文件的 xxh64（流式哈希），用于 Done 后与发送端核对。
    hashes: StdMutex<HashMap<u32, u64>>,
    /// 断点续传 journal（没有 manifest 摘要时为 None）。
    journal: Option<Arc<journal::JournalWriter>>,
}

impl Session {
    fn progress(&self, force: bool) {
        let bytes = self.bytes_done.load(Ordering::Relaxed);
        {
            let mut state = self.progress_state.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            if !force
                && bytes.saturating_sub(state.1) < 8 * 1024 * 1024
                && now.duration_since(state.0) < Duration::from_millis(200)
            {
                return;
            }
            *state = (now, bytes);
        }
        let _ = self.events.send(TransferEvent::ReceiveProgress {
            session_id: self.id.clone(),
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.total_files,
            bytes_done: bytes,
            bytes_total: self.total_bytes,
        });
    }

    async fn write_chunk(&self, id: u32, offset: u64, data: &[u8]) -> Result<(), String> {
        let Some(rel) = self.rel_paths.get(&id).cloned() else {
            return Err(format!("未知文件 id {id}"));
        };

        let existing = self
            .open
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned();
        let part = match existing {
            Some(part) => part,
            None => {
                let final_path = self.root.join(&rel);
                if let Some(parent) = final_path.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
                }
                let part_path = part_path_for(&final_path);
                let file = OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&part_path)
                    .await
                    .map_err(|e| format!("创建临时文件失败 {}: {e}", part_path.display()))?;
                let fresh = Arc::new(Mutex::new(PartState {
                    file,
                    written: 0,
                    part_path,
                    final_path,
                    hasher: Xxh64::new(0),
                }));
                self.open
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id, Arc::clone(&fresh));
                fresh
            }
        };

        let mut guard = part.lock().await;
        if guard.written != offset {
            return Err(format!("偏移不连续：期望 {} 实际 {offset}", guard.written));
        }
        guard
            .file
            .write_all(data)
            .await
            .map_err(|e| format!("写入失败 {rel}: {e}"))?;
        guard.hasher.update(data);
        guard.written += data.len() as u64;
        drop(guard);

        self.bytes_done
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        self.progress(false);
        Ok(())
    }

    async fn finish_file(&self, id: u32) -> Result<(), String> {
        if self
            .hashes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&id)
        {
            return Ok(());
        }
        let part = self
            .open
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        let Some(part) = part else {
            // 0 字节文件不会产生任何 DATA 帧，因此没有 PartState：直接建空文件并计入完成。
            if self.sizes.get(&id).copied() == Some(0) {
                return self.finish_empty_file(id).await;
            }
            return Ok(());
        };
        let mut guard = part.lock().await;
        guard
            .file
            .flush()
            .await
            .map_err(|e| format!("flush 失败: {e}"))?;
        let written = guard.written;
        let hash = guard.hasher.digest();
        let part_path = guard.part_path.clone();
        let final_path = guard.final_path.clone();
        // 不做逐文件 fsync：与 tar / robocopy 基线一致地只保证“写完 + 原子改名”。
        // 需要掉电持久性时由外层做一次目录级 checkpoint（见 AGENT.md 取舍）。
        drop(guard);

        if let Some(expected) = self.sizes.get(&id) {
            if written != *expected {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(format!("文件大小不符：期望 {expected} 实际 {written}"));
            }
        }
        tokio::fs::rename(&part_path, &final_path)
            .await
            .map_err(|e| format!("rename 失败 {}: {e}", final_path.display()))?;
        // 测试挂钩：LANPIPE_FAULT_DROP_HASH=1 时把首个文件的哈希故意写错，
        // 用来验证 Verify → Resync → 重传这条纠错链路（文件本身仍然落盘并计数）。
        let fault = std::env::var_os("LANPIPE_FAULT_DROP_HASH").is_some()
            && self.files_done.load(Ordering::Relaxed) == 0;
        let reported = if fault { hash ^ 0xA5A5_A5A5_A5A5_A5A5 } else { hash };
        self.hashes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, reported);
        if let Some(journal) = &self.journal {
            journal.record(id, written, hash);
        }

        let done = self.files_done.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(rel) = self.rel_paths.get(&id) {
            let _ = self.events.send(TransferEvent::FileReceived {
                session_id: self.id.clone(),
                path: rel.clone(),
                size: written,
            });
        }
        self.progress(false);
        if self.total_files > 0 && done == self.total_files {
            self.emit_received();
        }
        Ok(())
    }

    /// 0 字节文件：没有 DATA 帧，直接创建空文件（同样走 .lp-part → rename 流程）。
    async fn finish_empty_file(&self, id: u32) -> Result<(), String> {
        let Some(rel) = self.rel_paths.get(&id).cloned() else {
            return Ok(());
        };
        let final_path = self.root.join(&rel);
        if let Some(parent) = final_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
        }
        let part_path = part_path_for(&final_path);
        tokio::fs::write(&part_path, [])
            .await
            .map_err(|e| format!("创建临时文件失败 {}: {e}", part_path.display()))?;
        tokio::fs::rename(&part_path, &final_path)
            .await
            .map_err(|e| format!("rename 失败 {}: {e}", final_path.display()))?;
        let empty_hash = Xxh64::new(0).digest();
        self.hashes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, empty_hash);
        if let Some(journal) = &self.journal {
            journal.record(id, 0, empty_hash);
        }
        let done = self.files_done.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.events.send(TransferEvent::FileReceived {
            session_id: self.id.clone(),
            path: rel,
            size: 0,
        });
        self.progress(false);
        if self.total_files > 0 && done == self.total_files {
            self.emit_received();
        }
        Ok(())
    }

    /// 汇总已完成文件的哈希，交给发送端比对。
    fn verify_digest(&self) -> VerifyDigest {
        let hashes = self
            .hashes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, hash)| lanpipe_proto::FileHash {
                id: *id,
                hash: *hash,
            })
            .collect();
        VerifyDigest { hashes }
    }

    /// 收到重传请求：删掉这些文件的已落地内容与哈希，并回退计数。
    async fn resync(&self, ids: &[u32]) -> Result<(), String> {
        for &id in ids {
            self.open
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            let Some(rel) = self.rel_paths.get(&id).cloned() else {
                continue;
            };
            let final_path = self.root.join(&rel);
            let _ = tokio::fs::remove_file(&final_path).await;
            let _ = tokio::fs::remove_file(part_path_for(&final_path)).await;
            let had_hash = self
                .hashes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id)
                .is_some();
            // 哈希缺失但文件已落盘（异常路径）也要回退，否则重传后计数会多 1。
            let had_file = tokio::fs::metadata(&final_path).await.is_ok();
            if had_hash || had_file {
                self.files_done.fetch_sub(1, Ordering::Relaxed);
                let size = self.sizes.get(&id).copied().unwrap_or(0);
                self.bytes_done.fetch_sub(size, Ordering::Relaxed);
            }
        }
        self.progress(true);
        Ok(())
    }

    fn emit_received(&self) {
        if self
            .received_emitted
            .swap(true, Ordering::Relaxed)
        {
            return;
        }
        let _ = self.events.send(TransferEvent::Received {
            session_id: self.id.clone(),
            files: self.files_done.load(Ordering::Relaxed),
            bytes: self.bytes_done.load(Ordering::Relaxed),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            root: self.root.display().to_string(),
        });
    }
}


/// 调优 TCP：禁用 Nagle + 开 keepalive，避免长时间传输在虚拟网络/NAT 上静默卡死。
fn tune_stream(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    let sock = socket2::SockRef::from(stream);
    let keepalive = socket2::TcpKeepalive::new()
        .with_time(Duration::from_secs(30))
        .with_interval(Duration::from_secs(10));
    let _ = sock.set_tcp_keepalive(&keepalive);
}

fn part_path_for(final_path: &Path) -> PathBuf {
    let mut name = final_path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".lp-part");
    final_path.with_file_name(name)
}

/// 校验清单路径：拒绝绝对路径与 `..`。
/// 只接受单层目录名（不能是绝对路径、`..`、多级路径），否则返回 None。
fn safe_root_name(raw: &str) -> Option<String> {
    let path = safe_rel(raw)?;
    let mut components = path.components();
    let first = components.next()?;
    if components.next().is_some() {
        return None;
    }
    match first {
        Component::Normal(name) => {
            let value = name.to_string_lossy().to_string();
            if value.trim().is_empty() {
                None
            } else {
                Some(value)
            }
        }
        _ => None,
    }
}

fn new_root_name() -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("LanPipe-{stamp}")
}

fn safe_rel(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return None;
    }
    for c in p.components() {
        match c {
            Component::Normal(_) => {}
            _ => return None,
        }
    }
    Some(p.to_path_buf())
}

pub struct ReceiverHandle {
    pub local_addr: SocketAddr,
    tasks: Vec<JoinHandle<()>>,
}

impl ReceiverHandle {
    pub fn stop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

impl Drop for ReceiverHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

impl ReceiverHandle {
    fn from_tasks(local_addr: SocketAddr, tasks: Vec<JoinHandle<()>>) -> Self {
        Self { local_addr, tasks }
    }
}

/// 启动接收端：`bind` 通常为 `0.0.0.0:<tcp_port>`，收到文件直接写入 `dest_root/<root_name>/...`。
pub async fn start_receiver(
    bind: SocketAddr,
    dest_root: PathBuf,
) -> Result<(ReceiverHandle, mpsc::UnboundedReceiver<TransferEvent>), String> {
    let listener = TcpListener::bind(bind)
        .await
        .map_err(|e| format!("监听 {bind} 失败: {e}"))?;
    let local_addr = listener.local_addr().map_err(|e| e.to_string())?;
    tokio::fs::create_dir_all(&dest_root)
        .await
        .map_err(|e| format!("创建接收目录 {} 失败: {e}", dest_root.display()))?;

    let (tx, rx) = mpsc::unbounded_channel::<TransferEvent>();
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    let accept_events = tx.clone();
    let accept_sessions = Arc::clone(&sessions);
    let accept_task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let events = accept_events.clone();
                    let sessions = Arc::clone(&accept_sessions);
                    let dest = dest_root.clone();
                    tokio::spawn(async move {
                        // 单条连接出错（对端重启/半帧/磁盘临时错误）不应拖垮整个会话：
                        // 其余流继续传，缺失或损坏的文件由 Done 后的 Verify/Resync 补齐。
                        if let Err(err) = handle_conn(stream, sessions, events.clone(), dest).await {
                            eprintln!("[lanpipe] 连接结束（非致命）: {err}");
                        }
                    });
                }
                Err(err) => {
                    let _ = accept_events.send(TransferEvent::Failed {
                        session_id: None,
                        message: format!("accept 失败: {err}"),
                    });
                    time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    });

    Ok((
        ReceiverHandle::from_tasks(local_addr, vec![accept_task]),
        rx,
    ))
}

async fn handle_conn(
    mut stream: TcpStream,
    sessions: Sessions,
    events: mpsc::UnboundedSender<TransferEvent>,
    dest_root: PathBuf,
) -> Result<(), String> {
    tune_stream(&stream);
    let Some((ty, _flags, _s, payload)) = read_frame(&mut stream, MAX_CONTROL_FRAME)
        .await
        .map_err(|e| format!("读取首帧失败: {e}"))?
    else {
        return Ok(());
    };

    let session: Arc<Session> = match ty {
        FrameType::Offer => {
            let offer: Offer =
                serde_json::from_slice(&payload).map_err(|e| format!("解析 Offer 失败: {e}"))?;
            if offer.version != PROTOCOL_VERSION {
                let rej = serde_json::to_vec(&Reject {
                    reason: format!("协议版本不匹配: {}", offer.version),
                })
                .unwrap_or_default();
                let _ = write_frame(&mut stream, FrameType::Reject, 0, 0, &rej).await;
                return Ok(());
            }
            // 续传时复用上次的接收根目录（journal 头部记录），否则新开一个根。
            let digest = offer.manifest_digest.clone();
            let state_root = journal::default_state_dir();
            let journal_data = if digest.is_empty() {
                journal::JournalData::default()
            } else {
                journal::load(&state_root, &dest_root, &digest)
            };
            // root_name 也必须做单层校验：恶意发送端可用 `..` 或绝对路径把文件写到下载根之外。
            let root_name = journal_data
                .root_name
                .as_deref()
                .and_then(safe_root_name)
                .or_else(|| safe_root_name(&offer.root_name))
                .unwrap_or_else(new_root_name);
            let root = dest_root.join(&root_name);
            let mut rel_paths = HashMap::new();
            let mut sizes = HashMap::new();
            let mut total_bytes = 0u64;
            // 目录不在 Accept 前逐个创建：几万个目录在慢盘上要几十秒，会把发送端的 Accept 等待耗光。
            // 先只记账，Accept 之后后台补齐（文件写入时也会 create_dir_all 父目录）。
            let mut dir_rels: Vec<PathBuf> = Vec::new();
            for e in &offer.entries {
                let Some(rel) = safe_rel(&e.path) else {
                    let rej = serde_json::to_vec(&Reject {
                        reason: format!("非法路径: {}", e.path),
                    })
                    .unwrap_or_default();
                    let _ = write_frame(&mut stream, FrameType::Reject, 0, 0, &rej).await;
                    return Err(format!("非法路径: {}", e.path));
                };
                if e.is_dir {
                    dir_rels.push(rel);
                } else {
                    if rel_paths.insert(e.id, e.path.clone()).is_some() {
                        return Err(format!("清单中有重复 id: {}", e.id));
                    }
                    sizes.insert(e.id, e.size);
                    total_bytes += e.size;
                }
            }
            let total_files = sizes.len() as u64;
            let session_id = offer.session_id.clone();

            // 断点续传：按 manifest 摘要查 journal，只信任「目标文件存在且大小一致」的完成记录。
            let mut completed: Vec<u32> = Vec::new();
            let mut journal_hashes: HashMap<u32, u64> = HashMap::new();
            let mut done_bytes = 0u64;
            let mut journal_writer: Option<Arc<journal::JournalWriter>> = None;
            if !digest.is_empty() {
                let records = journal_data.records.clone();
                if let Ok(writer) =
                    journal::JournalWriter::open(&state_root, &dest_root, &digest, &root_name)
                {
                    journal_writer = Some(Arc::new(writer));
                }
                for (id, rec) in records {
                    let Some(size) = sizes.get(&id).copied() else {
                        continue;
                    };
                    if rec.size != size {
                        continue;
                    }
                    let Some(rel) = rel_paths.get(&id) else {
                        continue;
                    };
                    let target = root.join(rel);
                    if let Ok(meta) = tokio::fs::metadata(&target).await {
                        if meta.len() == size {
                            completed.push(id);
                            journal_hashes.insert(id, rec.xxh64);
                            done_bytes += size;
                        }
                    }
                }
                completed.sort_unstable();
                // 首版只做「已完成文件跳过」：上一轮中断留下的 .lp-part 清掉重传，
                // 单文件内部的偏移续传留给 L2。
                for (id, rel) in rel_paths.iter() {
                    if completed.contains(id) {
                        continue;
                    }
                    let _ = tokio::fs::remove_file(part_path_for(&root.join(rel))).await;
                }
            }
            let resume_state = if completed.is_empty() {
                None
            } else {
                Some(ResumeState {
                    completed: completed.clone(),
                    partial: Vec::new(),
                })
            };

            let sess = Arc::new(Session {
                id: session_id.clone(),
                root: root.clone(),
                rel_paths,
                sizes,
                total_files,
                total_bytes,
                started: Instant::now(),
                events: events.clone(),
                files_done: AtomicU64::new(completed.len() as u64),
                bytes_done: AtomicU64::new(done_bytes),
                open: StdMutex::new(HashMap::new()),
                progress_state: StdMutex::new((Instant::now(), done_bytes)),
                hashes: StdMutex::new(journal_hashes),
                journal: journal_writer,
                received_emitted: std::sync::atomic::AtomicBool::new(false),
            });
            let _ = events.send(TransferEvent::Incoming {
                session_id: session_id.clone(),
                root_name: root_name.clone(),
                files: total_files,
                bytes: total_bytes,
            });
            sessions.lock().await.insert(session_id.clone(), Arc::clone(&sess));
            sess.progress(true);
            let acc = serde_json::to_vec(&Accept {
                session_id: session_id.clone(),
                resume: resume_state,
            })
            .unwrap_or_default();
            write_frame(&mut stream, FrameType::Accept, 0, 0, &acc)
                .await
                .map_err(|e| format!("发送 Accept 失败: {e}"))?;
            if !dir_rels.is_empty() {
                let dir_root = root.clone();
                tokio::spawn(async move {
                    for rel in dir_rels {
                        let _ = tokio::fs::create_dir_all(dir_root.join(rel)).await;
                    }
                });
            }
            sess
        }
        FrameType::Hello => {
            let hello: Hello =
                serde_json::from_slice(&payload).map_err(|e| format!("解析 Hello 失败: {e}"))?;
            let found = sessions.lock().await.get(&hello.session_id).cloned();
            match found {
                Some(s) => s,
                None => {
                    let err = serde_json::to_vec(&ErrorMsg {
                        message: "会话不存在或已结束".into(),
                    })
                    .unwrap_or_default();
                    let _ = write_frame(&mut stream, FrameType::Error, 0, 0, &err).await;
                    return Ok(());
                }
            }
        }
        _ => {
            let err = serde_json::to_vec(&ErrorMsg {
                message: "需要 Offer 或 Hello 作为首帧".into(),
            })
            .unwrap_or_default();
            let _ = write_frame(&mut stream, FrameType::Error, 0, 0, &err).await;
            return Ok(());
        }
    };

    loop {
        let frame = read_frame(&mut stream, MAX_CONTROL_FRAME)
            .await
            .map_err(|e| format!("读取帧失败: {e}"))?;
        let Some((ty, _flags, _s, payload)) = frame else {
            return Ok(());
        };
        match ty {
            FrameType::Data => {
                if payload.len() < 12 {
                    return Err("DATA 帧过短".into());
                }
                if payload.len() > MAX_DATA_FRAME as usize {
                    return Err("DATA 帧超过协议上限".into());
                }
                let id = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let offset = u64::from_be_bytes([
                    payload[4], payload[5], payload[6], payload[7], payload[8], payload[9],
                    payload[10], payload[11],
                ]);
                session.write_chunk(id, offset, &payload[12..]).await?;
            }
            FrameType::FileDone => {
                let done: FileDone = serde_json::from_slice(&payload).unwrap_or(FileDone { id: 0 });
                session.finish_file(done.id).await?;
            }
            FrameType::Done => {
                let mut round = 0u32;
                loop {
                    round += 1;
                    // 只等 1s：跨连接的 FileDone 可能比 Done 晚到，等不到也没关系，
                    // 校验清单会让发送端发现缺失/不符的文件并按文件重传。
                    for _ in 0..20 {
                        if session.files_done.load(Ordering::Relaxed) == session.total_files {
                            break;
                        }
                        time::sleep(Duration::from_millis(50)).await;
                    }
                    let digest = session.verify_digest();
                    let payload = serde_json::to_vec(&digest).unwrap_or_default();
                    write_frame(&mut stream, FrameType::Verify, 0, 0, &payload)
                        .await
                        .map_err(|e| format!("发送校验清单失败: {e}"))?;
                    // 发送端裁决：Resync=按文件重传；Done=又一轮结束；Complete=校验通过。
                    let decision = loop {
                        let Some((ty, _f, _s, payload)) = read_frame(&mut stream, MAX_CONTROL_FRAME)
                            .await
                            .map_err(|e| format!("读取校验响应失败: {e}"))?
                        else {
                            return Ok(());
                        };
                        match ty {
                            FrameType::Resync => {
                                let rs: ResyncIds = serde_json::from_slice(&payload)
                                    .map_err(|e| format!("解析重传请求失败: {e}"))?;
                                session.resync(&rs.ids).await?;
                            }
                            FrameType::Done => break FrameType::Done,
                            FrameType::Complete => break FrameType::Complete,
                            FrameType::Error => {
                                let msg = serde_json::from_slice::<ErrorMsg>(&payload)
                                    .map(|m| m.message)
                                    .unwrap_or_else(|_| "对端错误".into());
                                return Err(msg);
                            }
                            _ => {}
                        }
                    };
                    if decision == FrameType::Complete {
                        if session.files_done.load(Ordering::Relaxed) == session.total_files {
                            session.emit_received();
                        } else {
                            let _ = session.events.send(TransferEvent::Failed {
                                session_id: Some(session.id.clone()),
                                message: format!(
                                    "校验通过但文件数不符：{}/{}",
                                    session.files_done.load(Ordering::Relaxed),
                                    session.total_files
                                ),
                            });
                        }
                        sessions.lock().await.remove(&session.id);
                        return Ok(());
                    }
                    if round >= 5 {
                        let _ = session.events.send(TransferEvent::Failed {
                            session_id: Some(session.id.clone()),
                            message: "校验重传次数超限".into(),
                        });
                        sessions.lock().await.remove(&session.id);
                        return Ok(());
                    }
                }
            }
            FrameType::Error => {
                let msg = serde_json::from_slice::<ErrorMsg>(&payload)
                    .map(|m| m.message)
                    .unwrap_or_else(|_| "对端错误".into());
                return Err(msg);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// 发送端
// ---------------------------------------------------------------------------

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 监视任务守卫：函数以任何方式返回（包括 `?` 提前 Err）都会 abort 掉它。
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone)]
struct SendItem {
    entry: ManifestEntry,
    source: PathBuf,
}

/// 把一批路径转换成一个清单；顶层名字去重。
fn collect_entries(paths: &[PathBuf]) -> Result<(String, Vec<SendItem>, u64), String> {
    let mut used_names: HashMap<String, u32> = HashMap::new();
    let mut items = Vec::new();
    let mut total = 0u64;
    let mut id = 0u32;

    for path in paths {
        if !path.exists() {
            return Err(format!("路径不存在: {}", path.display()));
        }
        let base = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "item".into());
        let count = used_names.entry(base.clone()).or_insert(0);
        *count += 1;
        let prefix = if *count == 1 {
            base
        } else {
            format!("{base}({count})")
        };

        if path.is_file() {
            let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
            items.push(SendItem {
                entry: ManifestEntry {
                    id,
                    path: prefix,
                    size: meta.len(),
                    mtime_ms: modified_ms(&meta),
                    is_dir: false,
                },
                source: path.clone(),
            });
            total += meta.len();
            id += 1;
            continue;
        }

        for item in walkdir::WalkDir::new(path).follow_links(false) {
            let item = item.map_err(|e| format!("遍历失败: {e}"))?;
            if item.path() == path {
                continue;
            }
            let rel = item
                .path()
                .strip_prefix(path)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let meta = item.metadata().map_err(|e| e.to_string())?;
            items.push(SendItem {
                entry: ManifestEntry {
                    id,
                    path: format!("{prefix}/{rel}"),
                    size: if meta.is_dir() { 0 } else { meta.len() },
                    mtime_ms: modified_ms(&meta),
                    is_dir: meta.is_dir(),
                },
                source: item.path().to_path_buf(),
            });
            if !meta.is_dir() {
                total += meta.len();
            }
            id += 1;
        }
    }
    if items.iter().all(|e| e.entry.is_dir) {
        return Err("没有可发送的文件".into());
    }
    let root_name = format!("LanPipe-{}", now_millis() / 1000);
    Ok((root_name, items, total))
}

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 发送一批路径到目标设备。
pub async fn send_paths(
    target: SocketAddr,
    paths: Vec<PathBuf>,
    streams: usize,
    events: mpsc::UnboundedSender<TransferEvent>,
) -> Result<(), String> {

    let streams = streams.clamp(1, 8);
    let (root_name, items, total_bytes) = collect_entries(&paths)?;
    let total_files = items.iter().filter(|i| !i.entry.is_dir).count() as u64;
    let session_id = uuid::Uuid::new_v4().to_string();

    let mut control = TcpStream::connect(target)
        .await
        .map_err(|e| format!("连接 {target} 失败: {e}"))?;
    tune_stream(&control);

    let manifest_entries: Vec<ManifestEntry> = items.iter().map(|i| i.entry.clone()).collect();
    let manifest_digest = journal::manifest_digest(&manifest_entries);
    let offer = Offer {
        version: PROTOCOL_VERSION,
        session_id: session_id.clone(),
        root_name,
        entries: manifest_entries,
        total_bytes,
        manifest_digest,
    };
    let payload = serde_json::to_vec(&offer).map_err(|e| e.to_string())?;
    write_frame(&mut control, FrameType::Offer, 0, 0, &payload)
        .await
        .map_err(|e| format!("发送 Offer 失败: {e}"))?;

    let reply = time::timeout(Duration::from_secs(900), read_frame(&mut control, MAX_CONTROL_FRAME))
        .await
        .map_err(|_| "等待接收端响应超时".to_string())?
        .map_err(|e| format!("读取响应失败: {e}"))?
        .ok_or_else(|| "接收端关闭连接".to_string())?;
    match reply.0 {
        FrameType::Accept => {}
        FrameType::Reject => {
            let reason = serde_json::from_slice::<Reject>(&reply.3)
                .map(|r| r.reason)
                .unwrap_or_else(|_| "接收端拒绝".into());
            let _ = events.send(TransferEvent::Failed {
                session_id: Some(session_id.clone()),
                message: reason.clone(),
            });
            return Err(reason);
        }
        _ => return Err("接收端返回了非预期响应".into()),
    }

    // 接收端可能返回可复用进度：跳过已完成文件，其余照常发送。
    let accepted: Option<Accept> = serde_json::from_slice(&reply.3).ok();
    let completed: std::collections::HashSet<u32> = accepted
        .and_then(|a| a.resume)
        .map(|r| r.completed.into_iter().collect())
        .unwrap_or_default();
    let skipped_files = completed.len() as u64;
    let skipped_bytes: u64 = items
        .iter()
        .filter(|i| !i.entry.is_dir && completed.contains(&i.entry.id))
        .map(|i| i.entry.size)
        .sum();

    let mut control = Some(control);
    let hashes: Arc<StdMutex<HashMap<u32, u64>>> = Arc::new(StdMutex::new(HashMap::new()));
    let items_map: Arc<HashMap<u32, SendItem>> = Arc::new(
        items
            .iter()
            .filter(|i| !i.entry.is_dir)
            .cloned()
            .map(|i| (i.entry.id, i))
            .collect(),
    );
    let data_items: Vec<SendItem> = items
        .iter()
        .filter(|i| !i.entry.is_dir && !completed.contains(&i.entry.id))
        .cloned()
        .collect();
    let bytes_done = Arc::new(AtomicU64::new(skipped_bytes));
    let files_done = Arc::new(AtomicU64::new(skipped_files));
    let started = Instant::now();

    let mon_events = events.clone();
    let mon_id = session_id.clone();
    let mon_bytes = Arc::clone(&bytes_done);
    let mon_files = Arc::clone(&files_done);
    let monitor = AbortOnDrop(tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_millis(200)).await;
            let _ = mon_events.send(TransferEvent::SendProgress {
                session_id: mon_id.clone(),
                files_done: mon_files.load(Ordering::Relaxed),
                files_total: total_files,
                bytes_done: mon_bytes.load(Ordering::Relaxed),
                bytes_total: total_bytes,
            });
        }
    }));

    let tasks = spawn_data_streams(
        target,
        session_id.clone(),
        data_items,
        streams,
        control.take(),
        Arc::clone(&bytes_done),
        Arc::clone(&files_done),
        Arc::clone(&hashes),
    );
    for t in tasks {
        t.await.map_err(|e| format!("发送任务异常: {e}"))??;
    }
    time::sleep(Duration::from_millis(150)).await;

    // Done → 接收端回 Verify（逐文件 xxh64）→ 不一致就按文件重传，最多 3 轮。
    let mut done_conn = TcpStream::connect(target)
        .await
        .map_err(|e| format!("连接接收端失败: {e}"))?;
    tune_stream(&done_conn);
    let hello = serde_json::to_vec(&Hello {
        session_id: session_id.clone(),
        stream: 0,
    })
    .unwrap_or_default();
    write_frame(&mut done_conn, FrameType::Hello, 0, 0, &hello)
        .await
        .map_err(|e| e.to_string())?;

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        write_frame(&mut done_conn, FrameType::Done, 0, 0, b"")
            .await
            .map_err(|e| e.to_string())?;
        let (ty, _f, _s, payload) = time::timeout(
            Duration::from_secs(900),
            read_frame(&mut done_conn, MAX_CONTROL_FRAME),
        )
        .await
        .map_err(|_| "等待接收端校验超时".to_string())?
        .map_err(|e| format!("读取校验响应失败: {e}"))?
        .ok_or_else(|| "接收端提前关闭连接".to_string())?;
        match ty {
            FrameType::Verify => {
                let digest: VerifyDigest = serde_json::from_slice(&payload)
                    .map_err(|e| format!("解析校验清单失败: {e}"))?;
                let local = hashes.lock().unwrap_or_else(|e| e.into_inner()).clone();
                let mut bad: Vec<u32> = Vec::new();
                for fh in &digest.hashes {
                    match local.get(&fh.id) {
                        Some(h) if *h == fh.hash => {}
                        // 断点续传跳过的文件发送端没有重算哈希，信任接收端 journal 记录。
                        None if completed.contains(&fh.id) => {}
                        _ => bad.push(fh.id),
                    }
                }
                let reported: std::collections::HashSet<u32> =
                    digest.hashes.iter().map(|h| h.id).collect();
                for id in local.keys() {
                    if !reported.contains(id) && !bad.contains(id) {
                        bad.push(*id);
                    }
                }
                if bad.is_empty() {
                    write_frame(&mut done_conn, FrameType::Complete, 0, 0, b"")
                        .await
                        .map_err(|e| e.to_string())?;
                    break;
                }
                if attempt >= 3 {
                    let msg = format!(
                        "校验未通过：重传 {} 次后仍有 {} 个文件不一致",
                        attempt - 1,
                        bad.len()
                    );
                    let _ = events.send(TransferEvent::Failed {
                        session_id: Some(session_id.clone()),
                        message: msg.clone(),
                    });
                    return Err(msg);
                }
                let rs = serde_json::to_vec(&ResyncIds { ids: bad.clone() })
                    .map_err(|e| e.to_string())?;
                write_frame(&mut done_conn, FrameType::Resync, 0, 0, &rs)
                    .await
                    .map_err(|e| format!("发送重传请求失败: {e}"))?;
                let retry_items: Vec<SendItem> = bad
                    .iter()
                    .filter_map(|id| items_map.get(id).cloned())
                    .collect();
                let tasks = spawn_data_streams(
                    target,
                    session_id.clone(),
                    retry_items,
                    streams,
                    None,
                    Arc::clone(&bytes_done),
                    Arc::clone(&files_done),
                    Arc::clone(&hashes),
                );
                for t in tasks {
                    t.await.map_err(|e| format!("重传任务异常: {e}"))??;
                }
                time::sleep(Duration::from_millis(150)).await;
            }
            FrameType::Error => {
                let msg = serde_json::from_slice::<ErrorMsg>(&payload)
                    .map(|m| m.message)
                    .unwrap_or_else(|_| "接收端错误".into());
                return Err(msg);
            }
            _ => return Err("接收端返回了非预期校验响应".into()),
        }
    }

    drop(monitor);
    let _ = events.send(TransferEvent::Sent {
        session_id,
        files: total_files,
        bytes: total_bytes,
        elapsed_ms: started.elapsed().as_millis() as u64,
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn spawn_data_streams(
    target: SocketAddr,
    session_id: String,
    items: Vec<SendItem>,
    streams: usize,
    control: Option<TcpStream>,
    bytes_done: Arc<AtomicU64>,
    files_done: Arc<AtomicU64>,
    hashes: Arc<StdMutex<HashMap<u32, u64>>>,
) -> Vec<JoinHandle<Result<(), String>>> {
    let queue: Arc<Mutex<VecDeque<SendItem>>> = Arc::new(Mutex::new(items.into_iter().collect()));
    let mut control = control;
    let mut tasks = Vec::new();
    for stream_index in 0..streams {
        let queue = Arc::clone(&queue);
        let bytes_done = Arc::clone(&bytes_done);
        let files_done = Arc::clone(&files_done);
        let hashes = Arc::clone(&hashes);
        let sid = session_id.clone();
        let control = if stream_index == 0 { control.take() } else { None };
        tasks.push(tokio::spawn(async move {
            let mut conn = match control {
                Some(c) => c,
                None => {
                    let mut c = TcpStream::connect(target)
                        .await
                        .map_err(|e| format!("建立数据连接失败: {e}"))?;
                    tune_stream(&c);
                    let hello = serde_json::to_vec(&Hello {
                        session_id: sid.clone(),
                        stream: stream_index as u16,
                    })
                    .unwrap_or_default();
                    write_frame(&mut c, FrameType::Hello, 0, stream_index as u16, &hello)
                        .await
                        .map_err(|e| format!("发送 Hello 失败: {e}"))?;
                    c
                }
            };
            loop {
                let item = {
                    let mut q = queue.lock().await;
                    q.pop_front()
                };
                let Some(item) = item else { break };
                let mut file = File::open(&item.source)
                    .await
                    .map_err(|e| format!("打开 {} 失败: {e}", item.source.display()))?;
                let mut buf = vec![0u8; DATA_CHUNK];
                let mut offset = 0u64;
                let mut hasher = Xxh64::new(0);
                loop {
                    let n = file
                        .read(&mut buf)
                        .await
                        .map_err(|e| format!("读取失败: {e}"))?;
                    if n == 0 {
                        break;
                    }
                    hasher.update(&buf[..n]);
                    let mut payload = Vec::with_capacity(12 + n);
                    payload.extend_from_slice(&item.entry.id.to_be_bytes());
                    payload.extend_from_slice(&offset.to_be_bytes());
                    payload.extend_from_slice(&buf[..n]);
                    time::timeout(
                        Duration::from_secs(900),
                        write_frame(
                            &mut conn,
                            FrameType::Data,
                            stream_index as u8,
                            stream_index as u16,
                            &payload,
                        ),
                    )
                    .await
                    .map_err(|_| "发送数据超时（连接可能已断开）".to_string())?
                    .map_err(|e| format!("发送数据失败: {e}"))?;
                    offset += n as u64;
                    bytes_done.fetch_add(n as u64, Ordering::Relaxed);
                }
                hashes
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(item.entry.id, hasher.digest());
                let done =
                    serde_json::to_vec(&FileDone { id: item.entry.id }).unwrap_or_default();
                write_frame(
                    &mut conn,
                    FrameType::FileDone,
                    stream_index as u8,
                    stream_index as u16,
                    &done,
                )
                .await
                .map_err(|e| format!("发送 FileDone 失败: {e}"))?;
                files_done.fetch_add(1, Ordering::Relaxed);
            }
            Ok::<(), String>(())
        }));
    }
    tasks
}

#[cfg(test)]
mod root_name_tests {
    use super::{new_root_name, safe_root_name};

    #[test]
    fn rejects_traversal_and_absolute_names() {
        assert_eq!(safe_root_name("LanPipe-123"), Some("LanPipe-123".to_string()));
        assert_eq!(safe_root_name("sub/dir"), None);
        assert_eq!(safe_root_name(".."), None);
        assert_eq!(safe_root_name("../escape"), None);
        assert_eq!(safe_root_name("C:\\Windows"), None);
        assert_eq!(safe_root_name(""), None);
        assert_eq!(safe_root_name("   "), None);
    }

    #[test]
    fn fallback_name_is_single_component() {
        let name = new_root_name();
        assert!(name.starts_with("LanPipe-"));
        assert_eq!(safe_root_name(&name), Some(name));
    }
}
