//! 纠错链路自测：故意漏掉一个文件的哈希，验证 Verify → Resync → 按文件重传 → Complete。

use std::path::{Path, PathBuf};
use std::time::Duration;

use lanpipe_net::{send_paths, start_receiver, TransferEvent};
use tokio::time::timeout;

fn temp_root(tag: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("lanpipe-resync-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, bytes: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, vec![b'r'; bytes]).unwrap();
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn corrupted_hash_triggers_resync_and_recovers() {
    std::env::set_var("LANPIPE_FAULT_DROP_HASH", "1");

    let src = temp_root("src");
    let dest = temp_root("dest");
    for d in 0..4 {
        for f in 0..10 {
            write_file(&src.join(format!("d{d}")).join(format!("f{f}.bin")), 4096 + f * 100);
        }
    }
    write_file(&src.join("zero.bin"), 0);

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

    let src_for_send = src.clone();
    let (tx, mut send_events) = tokio::sync::mpsc::unbounded_channel();
    let send = tokio::spawn(async move { send_paths(addr, vec![src_for_send], 2, tx).await });

    let mut received = false;
    let mut sent = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while !(received && sent) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "纠错传输超时（received={received} sent={sent}）"
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
    timeout(Duration::from_secs(30), send)
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
    assert_eq!(roots.len(), 1);
    assert_eq!(snapshot(&roots[0]), expected, "重传后文件树应完全一致");

    std::env::remove_var("LANPIPE_FAULT_DROP_HASH");
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_dir_all(&dest);
}
