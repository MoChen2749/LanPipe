//! 断点续传自测：第一次传输中途掐断接收端，重新启动后应跳过已完成文件并补齐剩余文件。

use std::path::{Path, PathBuf};
use std::time::Duration;

use lanpipe_net::{send_paths, start_receiver, TransferEvent};

fn temp_root(tag: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("lanpipe-resume-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, bytes: usize) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, vec![b'q'; bytes]).unwrap();
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
async fn resume_skips_completed_files_and_finishes() {
    let state = temp_root("state");
    std::env::set_var("LANPIPE_STATE_DIR", &state);

    let src = temp_root("src");
    let dest = temp_root("dest");
    for d in 0..3 {
        for f in 0..10 {
            write_file(
                &src.join(format!("d{d}")).join(format!("f{f}.bin")),
                128 * 1024 + f * 1024,
            );
        }
    }
    let src_name = src.file_name().unwrap().to_string_lossy().to_string();
    let expected: Vec<(String, u64)> = snapshot(&src)
        .into_iter()
        .map(|(p, s)| (format!("{src_name}/{p}"), s))
        .collect();

    // 第一次：传到一部分就掐断接收端。
    let (mut receiver, mut events) = start_receiver("127.0.0.1:0".parse().unwrap(), dest.clone())
        .await
        .expect("启动接收端");
    let addr = receiver.local_addr;
    let src_for_send = src.clone();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let send1 = tokio::spawn(async move { send_paths(addr, vec![src_for_send], 2, tx).await });

    let mut finished = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while finished < 8 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "第一次传输没有产生足够的完成事件"
        );
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Some(TransferEvent::FileReceived { .. })) => finished += 1,
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => break,
        }
    }
    receiver.stop();
    send1.abort();
    let _ = send1.await;
    drop(events); // 确保接收端事件通道关闭
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 第二次：新接收端（新端口），同一目标目录，应复用 journal。
    let (mut receiver2, mut events2) =
        start_receiver("127.0.0.1:0".parse().unwrap(), dest.clone())
            .await
            .expect("重启接收端");
    let addr2 = receiver2.local_addr;
    let src_for_send2 = src.clone();
    let (tx2, mut send_events) = tokio::sync::mpsc::unbounded_channel();
    let send2 = tokio::spawn(async move { send_paths(addr2, vec![src_for_send2], 2, tx2).await });

    let mut first_progress_files: Option<u64> = None;
    let mut received = false;
    let mut sent = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while !(received && sent) {
        assert!(tokio::time::Instant::now() < deadline, "续传超时");
        tokio::select! {
            ev = events2.recv() => {
                match ev {
                    Some(TransferEvent::ReceiveProgress { files_done, .. }) if first_progress_files.is_none() => {
                        first_progress_files = Some(files_done);
                    }
                    Some(TransferEvent::Received { files, .. }) => {
                        assert_eq!(files, expected.len() as u64);
                        received = true;
                    }
                    None => break,
                    _ => {}
                }
            }
            ev = send_events.recv() => {
                if let Some(TransferEvent::Sent { files, .. }) = ev {
                    assert_eq!(files, expected.len() as u64);
                    sent = true;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
    tokio::time::timeout(Duration::from_secs(30), send2)
        .await
        .expect("续传发送超时")
        .expect("发送任务 panic")
        .expect("续传发送失败");
    receiver2.stop();

    // 接收根是一层包装目录（LanPipe-<时间戳>），比较其内部文件树。
    let roots: Vec<PathBuf> = std::fs::read_dir(&dest)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(roots.len(), 1, "续传后应只有一个接收根目录: {roots:?}");
    assert_eq!(snapshot(&roots[0]), expected, "续传后文件树应与源一致");
    let skipped = first_progress_files.expect("续传应上报初始进度");
    assert!(
        skipped >= 8,
        "续传进度应从已完成文件数起步，实际 {skipped}"
    );
    let journal_dir = state.join("journals");
    assert!(
        std::fs::read_dir(&journal_dir).map(|d| d.count() > 0).unwrap_or(false),
        "应生成 journal 文件"
    );
}
