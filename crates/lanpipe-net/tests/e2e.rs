//! 端到端自测：真实 TCP 环回传输 + 文件树一致性 + 零暂存校验。

use std::path::{Path, PathBuf};
use std::time::Duration;

use lanpipe_net::{send_paths, start_receiver, TransferEvent};
use tokio::time::timeout;

fn temp_root(tag: &str) -> PathBuf {
    // 工作区内的二进制受沙箱限制只能写工作区，这里用 cargo 提供的 target/tmp 作根。
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("lanpipe-e2e-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, bytes: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, vec![b'x'; bytes]).unwrap();
}

/// 递归收集相对路径 → 大小。
fn snapshot(root: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for item in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if item.path() == root || item.file_type().is_dir() {
            continue;
        }
        let rel = item
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        out.push((rel, item.metadata().unwrap().len()));
    }
    out.sort();
    out
}

fn has_archive_or_part(root: &Path) -> bool {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.ends_with(".lp-part") || name.ends_with(".zip") || name.ends_with(".tar")
        })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_directory_transfer_zero_staging() {
    let src = temp_root("src");
    let dest = temp_root("dest");

    // 12 个目录 × 25 个小文件 + 2 个中等文件 + 1 层深目录
    for d in 0..12 {
        for f in 0..25 {
            let size = 1024 + ((d * 31 + f * 17) % 8192) as usize;
            write_file(&src.join(format!("dir{d:02}")).join(format!("file{f:02}.bin")), size);
        }
    }
    write_file(&src.join("big-a.bin"), 262_144);
    // 0 字节文件 + 空目录：曾经因为“没有 DATA 帧”而丢文件、且 files_done 收不齐。
    write_file(&src.join("server/permissions.yml"), 0);
    std::fs::create_dir_all(src.join("emptydir")).unwrap();
    write_file(&src.join("nested/deep/big-b.bin"), 524_288);

    let src_name = src.file_name().unwrap().to_string_lossy().to_string();
    let expected: Vec<(String, u64)> = snapshot(&src)
        .into_iter()
        .map(|(p, s)| (format!("{src_name}/{p}"), s))
        .collect();
    let expected_files = expected.len() as u64;
    let expected_bytes = expected.iter().map(|(_, s)| *s).sum::<u64>();

    let (receiver, mut events) = start_receiver("127.0.0.1:0".parse().unwrap(), dest.clone())
        .await
        .expect("启动接收端");
    let addr = receiver.local_addr;

    let (tx, mut send_events) = tokio::sync::mpsc::unbounded_channel();
    let src_for_send = src.clone();
    let send = tokio::spawn(async move { send_paths(addr, vec![src_for_send], 4, tx).await });

    let mut received = false;
    let mut sent = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while !(received && sent) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "传输超时（received={received} sent={sent}）"
        );
        tokio::select! {
            ev = events.recv() => {
                if let Some(TransferEvent::Received { files, bytes, .. }) = ev {
                    assert_eq!(files, expected_files);
                    assert_eq!(bytes, expected_bytes);
                    received = true;
                }
            }
            ev = send_events.recv() => {
                if let Some(TransferEvent::Sent { files, bytes, .. }) = ev {
                    assert_eq!(files, expected_files);
                    assert_eq!(bytes, expected_bytes);
                    sent = true;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
    let send_result = timeout(Duration::from_secs(30), send)
        .await
        .expect("发送任务超时")
        .expect("发送任务 panic");
    send_result.expect("发送失败");

    // 等接收端完成最后的 rename
    tokio::time::sleep(Duration::from_millis(300)).await;

    let roots: Vec<PathBuf> = std::fs::read_dir(&dest)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(roots.len(), 1, "应只有一个接收根目录: {roots:?}");
    let got = snapshot(&roots[0]);
    assert_eq!(got, expected, "文件树不一致");
    assert!(!has_archive_or_part(&roots[0]), "不应留下 .lp-part 或归档文件");

    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&dest);
}

/// 大量小文件的回归测试：Done 帧走独立连接，接收端必须先等数据连接的
/// DATA/FileDone 全部处理完再发校验清单。否则发送端会把“还在队列里的文件”
/// 误判为缺失并重传，与旧帧并发写同一个文件（Windows 上表现为 rename
/// 拒绝访问、残留 .lp-part、甚至连接被重置）。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn e2e_many_small_files_no_verify_race() {
    let src = temp_root("small-src");
    let dest = temp_root("small-dest");

    const DIRS: usize = 40;
    const FILES_PER_DIR: usize = 150;
    for d in 0..DIRS {
        for f in 0..FILES_PER_DIR {
            write_file(
                &src.join(format!("d{d:02}")).join(format!("f{f:03}.bin")),
                128,
            );
        }
    }

    let src_name = src.file_name().unwrap().to_string_lossy().to_string();
    let expected: Vec<(String, u64)> = snapshot(&src)
        .into_iter()
        .map(|(p, s)| (format!("{src_name}/{p}"), s))
        .collect();
    let expected_files = expected.len() as u64;
    let expected_bytes = expected.iter().map(|(_, s)| *s).sum::<u64>();

    let (receiver, mut events) = start_receiver("127.0.0.1:0".parse().unwrap(), dest.clone())
        .await
        .expect("启动接收端");
    let addr = receiver.local_addr;

    let (tx, mut send_events) = tokio::sync::mpsc::unbounded_channel();
    let src_for_send = src.clone();
    let send = tokio::spawn(async move { send_paths(addr, vec![src_for_send], 4, tx).await });

    let mut received = false;
    let mut sent = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while !(received && sent) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "传输超时（received={received} sent={sent}）"
        );
        tokio::select! {
            ev = events.recv() => {
                if let Some(TransferEvent::Received { files, bytes, .. }) = ev {
                    assert_eq!(files, expected_files);
                    assert_eq!(bytes, expected_bytes);
                    received = true;
                }
            }
            ev = send_events.recv() => {
                if let Some(TransferEvent::Sent { files, bytes, .. }) = ev {
                    assert_eq!(files, expected_files);
                    assert_eq!(bytes, expected_bytes);
                    sent = true;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
    timeout(Duration::from_secs(60), send)
        .await
        .expect("发送任务超时")
        .expect("发送任务 panic")
        .expect("发送失败");

    tokio::time::sleep(Duration::from_millis(300)).await;

    let roots: Vec<PathBuf> = std::fs::read_dir(&dest)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(roots.len(), 1, "应只有一个接收根目录: {roots:?}");
    let got = snapshot(&roots[0]);
    assert_eq!(got, expected, "文件树不一致");
    assert!(!has_archive_or_part(&roots[0]), "不应留下 .lp-part");

    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&dest);
}

