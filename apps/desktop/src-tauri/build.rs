fn main() {
    tauri_build::build();

    // 编译时间戳：用于确认用户运行的是哪一版（Win11 侧排查"改了没生效"）
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(out_dir.join("built_at.txt"), secs.to_string());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../dist");
    println!("cargo:rerun-if-changed=tauri.conf.json");
}
