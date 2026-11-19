# LanPipe 跨机测试报告（2026-11-19）

## 1. 测试环境

| 端 | 环境 |
| --- | --- |
| 发送/接收 A | Windows Server 2022（NT 10.0.20348），Xeon E5-2680 v4 / 28 线程 / 64 GB，F: 双 HDD 镜像卷（≈125 MiB/s 顺序写） |
| 发送/接收 B | WSL2 Ubuntu 24.04（lanpipe-cli，与 A 同一份 Rust 源码交叉构建） |
| 网络 | A ↔ B 走 WSL 虚拟网卡（NAT，172.18.x）；**不是真实 1GbE 物理链路** |
| 防火墙 | Windows 入站规则 `LanPipe TCP/UDP in (LAN)`，`-RemoteAddress LocalSubnet`（TCP/UDP 53310-53311、53320-53321） |

> 说明：WSL 虚拟网络用于**功能验证**；真实吞吐上限需要两台物理机同网段复测（见 §6）。

## 2. 构建与部署

| 项 | 命令 | 结果 |
| --- | --- | --- |
| Windows 单 exe | `build.bat build`（等价 `cargo build --release --features custom-protocol -p lanpipe`） | `lanpipe.exe` 7,565,312 B |
| Linux CLI | WSL: `cargo build --release -p lanpipe-cli` | `lanpipe-cli` 865,440 B |
| 单元/集成测试 | `cargo test -p lanpipe-net` | `e2e_directory_transfer_zero_staging`、`corrupted_hash_triggers_resync_and_recovers` 均通过 |

## 3. 功能测试矩阵

| # | 功能 | 方法 | 结果 |
| --- | --- | --- | --- |
| 1 | 自动发现（双向） | A 起 GUI、B 起 `lanpipe-cli recv`，观察双方日志 | ✅ A 日志 `peer up WSL-Ubuntu 172.18.249.169:53311`；B 日志 `PEER-UP WIN-2H1782KNJDF 172.18.240.1:53311` |
| 2 | 发现刷新 | 窗口后启动 / 手动刷新 / 4s 轮询 | ✅ UI 设备列表出现对端；`peers:update` 每 2s 重推 |
| 3 | A → B 传输（含 0 字节文件） | `开服侠（生存）` 128 文件 / 91.6 MiB | ✅ 370–500 ms；B 端 128 文件、字节数一致、`server/permissions.yml` 0 字节保留、无 `.lp-part` |
| 4 | B → A 传输 | `1.12.2` 488 文件 / 510 MiB | ✅ 5.6 s（≈92.6 MiB/s，受 B 读 `/mnt/f` 的 9p 开销限制）；文件树一致 |
| 5 | IP 直连 | A 直接发 `172.18.249.169:53311`，不走发现 | ✅ 成功 |
| 6 | IP 尾号扫描 | `LanPipe.exe --scan 5` / `--scan 11` | ✅ 找到 `192.168.100.5`、`192.168.100.11` 等候选 |
| 7 | 纠错重传 | B 端 `LANPIPE_FAULT_DROP_HASH=1` 注入坏哈希 | ✅ `Verify → Resync → 重传 → Complete`，128/128 一致，`RECEIVED` 只上报 1 次，0 字节文件保留 |
| 8 | 接收进度 / 完成提示 | GUI 观察 | ✅ 「接收完成 100% · 488 个文件 · 510 MiB · 5.5s · 92.6 MiB/s」 |
| 9 | 并发流数（发送端设置） | `--streams 1/2/4/8` | ✅ 单调提升，4↔8 差异小；UI 文案已注明发送端设置 |
| 10 | 设置持久化 | 改名称/目录/端口后重启 | ✅ `%APPDATA%\LanPipe\config.json` 保持，坏文件回滚 `.bad` |
| 11 | 同机双实例 | `--instance b`：UDP 53310 共用、TCP 53321 | ✅ 互相发现；端口不再漂移 |
| 12 | 大目录压力（25 GB / 43,594 文件） | A → B 全量 `F:\MinecraftServer` | ✅ 通过（见 §4） |

## 4. 25 GB 全量压力测试

**结果：✅ 通过。** 4 流发送 `F:\MinecraftServer`（43,594 文件 / 25,001,024,890 B）→ WSL `/tmp/recv-full7`：

- 发送端：`cli-sent files=43594 bytes=25001024890 elapsed_ms=1313549`，wall ≈ **1334 s（22.3 min，平均 ≈18.7 MB/s）**。
- 接收端：`RECEIVED files=43594 bytes=25001024890`；逐文件清单（相对路径 + 大小，43,594 行）与源 **完全一致**；`.lp-part` 残留 0；6,354 个 0 字节文件一致。
- 速度说明：源盘是 2×HDD 镜像卷且目录含 4.3 万个小文件，瓶颈是 HDD 的 IOPS（顺序写实测 125 MB/s，随机小文件远低于此），不是网络；WSL2 虚拟链路不是瓶颈。

期间发现并修复：
1. Offer 清单约 5–6 MiB，超过 `MAX_DATA_FRAME=4 MiB` → 引入 `MAX_CONTROL_FRAME = 64 MiB`（DATA 帧仍限 4 MiB）；
2. 接收端在回 `Accept` 前逐个 `create_dir_all` 6,699 个目录，慢盘上超过 30 s → 发送端 Accept 超时；改为 Accept 后后台建目录；
3. 大清单序列化/反序列化爆栈 → CLI/GUI 发送路径放 64 MiB 栈线程、tokio worker 16 MiB；**真因是 `tune_stream` 被批量替换成自递归**（已修）；
4. DATA 写超时 60 s → 180 s → 900 s：慢盘抖动不再被误判为断连（首次 25 GB 在 43,539/43,594 文件、24.96/25.00 GB 处因 180 s 超时失败，调大后一次通过）；
5. 单连接异常只结束该连接（不再丢弃整个会话）、keepalive 30s/10s、`RECEIVED` 事件去重。

## 5. 本轮修复清单

| 问题 | 修复 |
| --- | --- |
| 0 字节文件不产生 DATA 帧 → 接收端丢文件、计数不齐 | `finish_empty_file`：建空文件、计数、发 `FileReceived` |
| 传输完整性 | 每文件流式 xxHash64 + `Verify / Resync / Complete`，最多 3 轮按文件重传 |
| 设备发现多网卡/VPN 只从一个虚拟网卡发公告 | 每网卡独立 socket（`set_multicast_if_v4`）+ 子网广播；`Announce.addrs` + `pick_peer_addr` |
| 窗口晚加载看不到设备 | 主进程每 2s 重推 `peers:update` + UI 刷新按钮 + 4s 轮询 |
| 同机双实例端口漂移 | 仅首次生成配置时偏移 TCP，且 UDP 发现端口保持 53310 |
| 控制帧超限 | `MAX_CONTROL_FRAME = 64 MiB`，DATA 帧仍限 4 MiB |
| 长连接静默卡死 | keepalive 30s/10s + DATA 写入 900s 超时 + 单连接异常隔离 |
| 大清单爆栈 | 64 MiB 栈线程 + tokio 16 MiB；`tune_stream` 自递归修复 |
| 6,699 目录阻塞 Accept | Accept 后后台 `create_dir_all` |

## 6. 结论与下一步

- **功能链路已全部打通**：双向发现、双向传输、IP 直连、尾号扫描、0 字节文件、纠错重传、设置持久化、同机双实例。
- **吞吐数字只代表 WSL 虚拟链路**：B → A 约 92.6 MiB/s（9p 读取瓶颈），A → B 510 MiB 约 1.3 s。真实 1GbE / 2.5GbE 需要两台物理机复测。
- **25 GB 级目录已验证通过**（43,594 文件 / 25,001,024,890 B，逐文件清单一致）；平均 18.7 MB/s 是源 HDD 的 IOPS 限制，换 SSD 源会显著提升。
- 仍未验证：真实两台物理机 1GbE/2.5GbE 吞吐（本轮两端在 WSL2 虚拟链路上）；M1 建议加「会话级检查点 + 断点续传」以支持中断续跑。
- M1 计划：断点续传、防火墙一键放行、托盘、小文件批量合并、真实物理机吞吐基线。

## 7. 复现命令

Windows（发送）：

```powershell
& "$env:LOCALAPPDATA\Programs\LanPipe\LanPipe.exe" --send <目标IP>:53311 "F:\MinecraftServer\开服侠（生存）" --streams 4
& "$env:LOCALAPPDATA\Programs\LanPipe\LanPipe.exe" --scan 5
```

WSL Ubuntu（接收）：

```bash
./target-linux/release/lanpipe-cli recv --dest /tmp/recv --name WSL-Ubuntu
# 故障注入：LANPIPE_FAULT_DROP_HASH=1 ./target-linux/release/lanpipe-cli recv ...
```

防火墙（仅本地子网）：

```powershell
New-NetFirewallRule -DisplayName 'LanPipe TCP in (LAN)' -Direction Inbound -Action Allow `
  -Protocol TCP -LocalPort 53310-53311,53320-53321 -RemoteAddress LocalSubnet
New-NetFirewallRule -DisplayName 'LanPipe UDP in (LAN)' -Direction Inbound -Action Allow `
  -Protocol UDP -LocalPort 53310-53311,53320-53321 -RemoteAddress LocalSubnet
```
