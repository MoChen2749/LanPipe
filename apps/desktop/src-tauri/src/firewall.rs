//! Windows 防火墙放行：以管理员身份写入三条入站规则（程序 + UDP + TCP）。
//! 用 ShellExecuteW("runas") 触发 UAC，不要求整个应用以管理员运行。
//!
//! 实现细节：PowerShell 的命令行里嵌套引号很容易被吃掉，所以把脚本写到临时 .ps1，
//! 再用 `-File` 执行；启动后轮询规则状态，最多等 10 秒。

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::time::{Duration, Instant};

const RULE_PROGRAM: &str = "LanPipe";
const RULE_UDP: &str = "LanPipe-UDP";
const RULE_TCP: &str = "LanPipe-TCP";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[link(name = "shell32")]
extern "system" {
    fn IsUserAnAdmin() -> i32;

    fn ShellExecuteW(
        hwnd: isize,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

fn rule_exists(name: &str) -> bool {
    let output = std::process::Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            &format!("name={name}"),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match output {
        Ok(out) => out.status.success() && String::from_utf8_lossy(&out.stdout).contains(name),
        Err(_) => false,
    }
}

pub fn status() -> bool {
    rule_exists(RULE_PROGRAM) && rule_exists(RULE_UDP) && rule_exists(RULE_TCP)
}

fn script(program: &str, udp: u16, tcp: u16) -> String {
    format!(
        r#"$ErrorActionPreference = 'SilentlyContinue'
$name = '{RULE_PROGRAM}'
$udpRule = '{RULE_UDP}'
$tcpRule = '{RULE_TCP}'
$exe = '{program}'
netsh advfirewall firewall delete rule name="$name" | Out-Null
netsh advfirewall firewall delete rule name="$udpRule" | Out-Null
netsh advfirewall firewall delete rule name="$tcpRule" | Out-Null
netsh advfirewall firewall add rule name="$name" dir=in action=allow program="$exe" enable=yes profile=any | Out-Null
netsh advfirewall firewall add rule name="$udpRule" dir=in action=allow protocol=UDP localport={udp} enable=yes profile=any | Out-Null
netsh advfirewall firewall add rule name="$tcpRule" dir=in action=allow protocol=TCP localport={tcp} enable=yes profile=any | Out-Null
exit 0
"#
    )
}

/// 写入规则（会弹 UAC）；返回时若规则已生效则 Ok(true)。
pub fn allow(program: &Path, udp: u16, tcp: u16) -> Result<bool, String> {
    // 已经有了就直接返回，避免重复弹 UAC，也避免 delete/add 竞态
    if status() {
        return Ok(true);
    }
    let script_path = std::env::temp_dir().join("lanpipe-firewall.ps1");
    std::fs::write(&script_path, script(&program.display().to_string(), udp, tcp))
        .map_err(|err| format!("无法写入临时脚本：{err}"))?;

    let parameters = format!(
        "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{}\"",
        script_path.display()
    );
    let operation = wide("runas");
    let file = wide("powershell.exe");
    let parameters = wide(&parameters);
    let directory = wide("");
    let result = unsafe {
        ShellExecuteW(
            0,
            operation.as_ptr(),
            file.as_ptr(),
            parameters.as_ptr(),
            directory.as_ptr(),
            0,
        )
    };
    if (result as isize) <= 32 {
        let _ = std::fs::remove_file(&script_path);
        return Err(format!("需要管理员权限（ShellExecute={result}）"));
    }

    // 等 UAC 确认 + netsh 写规则（最多 12 秒）；规则要连续两次成立才算完成，
    // 避免脚本正在 delete/add 的中间态被误判。
    let started = Instant::now();
    let mut allowed = false;
    while started.elapsed() < Duration::from_secs(12) {
        if status() {
            std::thread::sleep(Duration::from_millis(700));
            if status() {
                allowed = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let _ = std::fs::remove_file(&script_path);
    if allowed {
        Ok(true)
    } else {
        Err("未检测到防火墙规则，可能取消了管理员授权".to_string())
    }
}
