# 断点续传设计调研（RESUME / M1-1）

> 状态：调研与选型文档（2026-11-20）。目的：先厘清「现有权威做法 + 论文/规范 + 可验证的取舍」，再动手改 LP/1 协议与接收端持久化。本文只做设计与证据，代码实现见后续 `feat: resume` 提交。
>
> 结论先行：**LanPipe 采用「会话级 journal + 文件级完成表」的 L1 方案**，不引入 rsync 式滚动哈希增量（L3），块级 bitmap 与单文件内部偏移续传作为 L2 扩展预留。理由与证据见 §5–§7。
>
> **实现状态（2026-11-20）**：L1 首版已落地，范围裁剪为「已完成文件跳过 + 复用上次接收根目录」；`.lp-part` 中断残留一律清掉重传，单文件内部偏移续传留待 L2。

---

## 1. 问题定义与约束

目标场景：一次发送 25 GB 级游戏目录（43,594 个文件，含 6,354 个 0 字节文件），LAN 内 1 GbE/2.5 GbE。中断来源：

| 中断类型 | 例子 |
| --- | --- |
| 网络抖动/切换 Wi-Fi | 发送端 `send_paths` 报连接错误 |
| 进程被杀/崩溃 | `taskkill /F`、OOM、panic |
| 接收端暂停或重启 | 主机休眠、LanPipe 被关闭 |
| 目标磁盘满/权限变化 | 写入失败 |

硬约束（延续前序决策）：

1. **接收端峰值额外磁盘 ≈ 0**：禁止「先落完整归档再解包」，因此续传只能基于已落盘的目录树 + 边角状态文件（journal），不能保留第二份数据。
2. **不能要求发送端全量预读**：发送端不应为了续传先把 25 GB 读一遍算哈希（会显著拉长首字节时间）。
3. **43k 文件规模**：状态表必须能 O(1) 查询与增量落盘，不能用「每次重传整个 manifest」的方式做恢复判断（5–6 MB 清单本身就要分帧）。
4. **可验证**：恢复后必须能证明文件树与源一致；不能只凭「大小相等」。
5. **跨平台**：Windows/macOS/Linux 一致行为；不依赖平台特有的文件系统特性（如 Windows USN、macOS clonefile）。

---

## 2. 现有实现的断点续传能力（横向证据）

| 系统/协议 | 粒度 | 机制 | 额外磁盘 | 局限 |
| --- | --- | --- | --- | --- |
| **tus.io 1.0.0** | 单文件字节偏移 | `HEAD` 返回 `Upload-Offset`；`PATCH` 带 `Upload-Offset` 续写；偏移不符返回 `409`（[规范](https://tus.io/protocols/resumable-upload)） | 服务端保存已收字节 | 仅 upload；无目录/manifest 语义；每文件一个资源 |
| **HTTP Range (RFC 9110)** | 单文件字节区间 | `Range: bytes=N-` + `206 Partial Content`（[RFC 9110 §14](https://www.rfc-editor.org/rfc/rfc9110)） | 无 | 需服务端支持；无校验/一致性 |
| **rsync**（`--partial`、`--inplace`） | 文件块 | 弱滚动校验（Adler-32 类）+ 强哈希定位差异块，只传 delta（Tridgell & Mackerras, *The rsync algorithm*, TR-CS-96-05；Tridgell 博士论文 1999） | 默认临时文件（`--partial` 保留部分） | 需读旧文件做校验和；目录树场景每文件一次 RTT/校验开销 |
| **BitTorrent (BEP-3)** | piece（通常 256 KiB–4 MiB） | 每 piece SHA-1；`bitfield` 声明已有片；断点靠本地已校验 piece（[BEP-3](https://www.bittorrent.org/beps/bep_0003.html)） | 已有 piece 即数据本身 | 面向多 peer 分发，单播场景过重；无文件元数据 |
| **Syncthing (BEP v1)** | block（128 KiB/256 KiB/512 KiB/1/2/4/8/16 MiB 八档） | 每 block SHA-256；`.tmp` 临时文件 + block 索引，重启后对临时文件做块校验续传（[BEP 规范](https://docs.syncthing.net/specs/bep-v1.html)、[同步原理](https://docs.syncthing.net/users/syncing.html)） | 临时文件 = 未完成数据（1x） | 需要扫盘/建索引；数万文件场景 CPU/IO 重；同步语义非「一次性发送」 |
| **iroh-blobs / sendme** | BLAKE3 chunk（bao 验证流） | 内容寻址 + 每 chunk 可验证；`FsStore` 保留 partial，恢复时按已验证前缀续传（[iroh docs](https://www.iroh.computer/docs)、[iroh-blobs](https://docs.rs/iroh-blobs)） | store 内 partial（1x） | 目录=HashSeq，每文件一 blob，无 POSIX 元数据；偏库/CLI |
| **croc** | 文件级 | 传输前协商，已有 partial 可续（README 声称支持断点续传） | partial 文件（1x） | 无目录树语义；大目录需打包 |
| **robocopy `/Z` `/ZB`** | 文件级 | 重启后从已写字节继续（[MS 文档](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/robocopy)） | 目标文件本身 | 仅 Windows/SMB；`/Z` 明显降速；无目录级清单校验 |
| **scp / LocalSend / 多数 GUI 工具** | 无 | 中断即整文件/整目录重来（LocalSend 无 resume，前序调研已记录） | — | 25 GB 场景不可接受 |

**证据小结**：

- 「按文件字节偏移续写」是 tus/HTTP Range/robocopy 的共同最小机制，实现便宜、与「零额外磁盘」天然兼容（已写字节就是目标文件的一部分）。
- 「按块校验续传」需要每块强哈希（BT/Syncthing/iroh），能在崩溃后验证已收数据，但要么引入块索引（Syncthing 扫盘重建），要么依赖内容寻址（iroh）。
- 「增量 delta」需要 rsync 式滚动哈希 + 读旧文件，适合「同一目录反复更新」，对「一次性发送新目录」收益低、成本高。

---

## 3. 学术与规范依据（可直接引用的点）

1. **rsync 算法**（Tridgell & Mackerras, TR-CS-96-05；Tridgell 1999 博士论文）：弱滚动校验 + 强哈希的块匹配，是「只传差异块」的经典方案；其代价是**接收端必须能读旧文件**并提供块校验和列表。对 LanPipe：新目录没有「旧文件」，收益仅体现在「中断后已收部分」——此时不需要滚动匹配，只需偏移。
2. **FastCDC**（Xia et al., USENIX ATC 2016, [论文](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf)）：内容定义分块（CDC）+ normalized chunking，解决固定分块在插入/删除后的边界漂移；用于去重/增量同步。LanPipe 的「发送新目录」场景不涉及内容偏移，CDC 属于 L3 才需要的优化。
3. **tus 1.0.0**（2016，[规范](https://tus.io/protocols/resumable-upload)）：把「续传」抽象为 `Upload-Offset` 状态机；关键设计是**服务端权威偏移 + 客户端按偏移续写 + 偏移冲突显式报错（409）**，而不是客户端猜。这一条直接可搬到 LP/1：接收端回 `partial: (id, offset)`，发送端从 offset 续写；offset 不连续就丢弃该文件重传。
4. **BitTorrent BEP-3**（[规范](https://www.bittorrent.org/beps/bep_0003.html)）：piece 级哈希 + bitfield，是「已收块可验证」的最简工程实现；但每 piece 哈希需要额外清单（约 size/4MiB × 20 B），43k 文件 × 平均 0.6 MB 场景反而放大状态体积，因此 LanPipe 只在**单文件 > 阈值**时启用块级（L2）。
5. **Syncthing BEP v1**（[规范](https://docs.syncthing.net/specs/bep-v1.html)）：块大小按文件大小自适应（128 KiB–16 MiB 八档）、每块 SHA-256、临时文件 + 重启重建块索引。它证明「块级续传 + 崩溃后校验」可行，但也说明成本：**重启要扫临时文件**，数万小文件时索引开销大（前序实测 Syncthing 1.2M 文件仅 ~5 KiB/s 量级）。
6. **iroh-blobs**（[docs](https://docs.rs/iroh-blobs)）：BLAKE3 + bao 的「可验证流」，每 chunk 可独立验证、前缀可截断续传；是「校验强度最高的库级方案」，但目录元数据需自建（sendme 的 HashSeq 每文件一 blob）。
7. **QUIC / TLS 恢复**（RFC 9000/9001）解决的是**连接**恢复（0-RTT、连接迁移），不解决**应用层已写字节的语义**；因此 LanPipe 的续传必须自己做，QUIC 只能降低重连成本。

---

## 4. 粒度选择：文件级 vs 块级

| 维度 | 文件级（L1） | 块级（L2） |
| --- | --- | --- |
| 恢复单位 | 已完成文件跳过；未完成文件从连续 offset 续写 | 任意已校验块可跳过；支持乱序/多流写入 |
| 状态体积 | 每文件 ~40 B（id/size/hash/state）；43k 文件 ≈ 1.7 MB | 每 4 MiB 一块 ≈ 20–32 B；25 GB ≈ 6.4k 块 ≈ 200 KB，但小文件放大 |
| 崩溃一致性 | 需信任「journal 记录 + 文件 size」；末次记录可能丢失，最多重传 1 个文件 | 每块校验可证明，恢复精度更高 |
| 实现成本 | 低（journal + Offer/Accept 扩展 + 发送端跳过逻辑） | 中（多流随机偏移、块 bitmap、接收端并发写偏移） |
| 与现有 LP/1 契合 | 高：已有 `id/offset` 语义、`.lp-part`、逐文件 xxh64 Verify/Resync | 中：需改 `write_chunk` 允许非连续 offset、块级 hash 表 |
| 25 GB 实际收益 | 中断后最多重传「当前文件」+ 已完成文件秒过 | 中断后最多重传「当前 4 MiB 块」 |

**结论**：先做 L1。25 GB / 43k 文件时，L1 的损失上限是「最后一个文件」（最大 5 GB 的 zip 可能较大，但 Minecraft 目录里最大单文件约 100–300 MB），相对 25 GB 可忽略；L2 留给「单文件 > 1 GB 且网络极不稳定」的场景。

---

## 5. LanPipe 推荐设计（L1）

### 5.1 状态存放（接收端，零额外磁盘）

```
<接收根>/
  .lanpipe/
    journals/
      <manifest_digest>.jsonl        # 追加写：{id,state,size,xxh64,offset,ts}
      <manifest_digest>.jsonl.tmp    # 压缩重写用
    sessions.json                    # 可选：sessionId → manifest_digest / 创建时间 / TTL
```

- `manifest_digest` = 对排序后的 `(path, size, mtime_ms, is_dir)` 做 **xxh3-128 / BLAKE3**（发送端算，随 Offer 下发；接收端只比对，不重算）。
- 每完成一个文件，追加一行 `{"id":..,"state":"done","xxh64":..,"size":..}`；每 1 s 或 32 MB 调一次 `sync_data`（不逐文件 fsync，延续 M0 取舍）。崩溃最多丢最近 1 s 的记录 → 恢复时对这些文件按 size/hash 复核或直接重传（代价 ≤ 1 s 数据）。
- `.lanpipe/` 加入接收端清理策略：TTL 7 天、sessions 上限 32 个、成功后保留 24 h 便于「同一目录重发秒过」。

### 5.2 协议扩展（LP/1 v1.1，向后兼容）

新增帧：

| 帧 | 方向 | 载荷 |
| --- | --- | --- |
| `ResumeQuery` | 发送端 → 接收端 | `{manifest_digest, total_files, total_bytes}`（先于 Offer） |
| `ResumeState` | 接收端 → 发送端 | `{found: bool, completed: [u32], partial: [(u32,u64)], dirs_done: bool}` |
| `Offer`（扩展字段） | 发送端 → 接收端 | 增加 `manifest_digest: String`、`resume_token: Option<String>` |

流程：

1. 发送端 `collect_entries` 同时算 `manifest_digest`（只哈希元数据，不读文件内容）。
2. 建连后先发 `ResumeQuery`；接收端查 journal：
   - 无记录 → `ResumeState{found:false}`，走现有全量流程；
   - 有记录 → 返回 `completed`/`partial`；发送端据此跳过已完成文件、对 partial 从 offset 续写（`DATA` 帧 offset 不要求 0 开始，接收端 `write_chunk` 改为允许 `offset == written`，即连续追加；不连续则整文件重传）。
3. 仍以现有 `Done → Verify → Resync → Complete` 收尾：**恢复后所有文件（含跳过的）都参与 Verify**。跳过的文件由接收端上报 journal 中的 xxh64，发送端与自身 manifest 中的 xxh64 比对（发送端在 collect 时对每个文件算 xxh64？成本高）。
   - 折中：**发送端只对「本次实际发送的文件」算 xxh64；跳过的文件由接收端上报 xxh64 + size，发送端信任 journal**（journal 是接收端自写，同机可信）；若需强校验，提供设置「恢复后全量校验（慢）」。
   - 这样保持「不预读全部文件」的约束。

### 5.3 发送端跳过与 partial 续写

- `send_paths` 接收 `ResumeState`：`skip: HashSet<u32>`（completed）与 `offsets: HashMap<u32,u64>`（partial）。
- 队列只放「未完成」文件；partial 文件从 `offset` 开始读（`File::seek`），`DATA.offset` 从该 offset 起；`bytes_done` 初始化为 `sum(partial offsets)`，进度不跳变。
- 若 `offset >= size`：视为已完成，跳过。
- 若接收端报告 `offset` 不连续（例如 `.lp-part` 中间有洞）：发送端从 0 重传该文件（接收端先 truncate）。

### 5.4 崩溃与一致性

| 场景 | 处理 |
| --- | --- |
| 发送端崩溃 | 接收端 journal 保留；重启后重新 `ResumeQuery`，按已收状态续传 |
| 接收端崩溃 | `.lp-part` 保留；journal 最后 1 s 记录可能丢失 → 该文件按 size 判断：`part.size < journal.offset` 不符则从 `part.size` 续；`part.size > size` 则截断 |
| 文件已 rename 但 journal 未落盘 | 恢复时「目标文件存在且 size 匹配」→ 视为完成并补写 journal（xxh64 可选重算） |
| journal 损坏 | JSONL 逐行解析，坏行丢弃并告警；极端情况整份删除重来（不影响已落盘文件，只是重传） |
| 目标磁盘满 | 暂停/失败；续传时按剩余空间计算可继续的文件，其余跳过并提示 |
| 源文件变化（mtime/size 变） | manifest_digest 不同 → 视为新会话，不复用旧 journal |

### 5.5 安全与清理

- journal 只存在接收根下，路径来自已校验 manifest（拒绝 `..`/绝对路径）。
- `ResumeState` 只暴露 id 集合与偏移，不泄露文件内容。
- `.lanpipe` 目录不参与「接收完成」计数与文件树比对（验证工具需排除）。

---

## 6. 方案对比矩阵（L1/L2/L3）

| 方案 | 恢复粒度 | 额外磁盘 | 实现成本 | 25 GB 恢复代价 | 风险 |
| --- | --- | --- | --- | --- | --- |
| **L1 文件级 + journal（推荐）** | 文件 / 单文件连续偏移 | ≈0（journal ~MB） | 3–5 人日 | 重传 ≤ 最后 1 个文件 | journal 一致性；partial offset 连续性 |
| L2 块级 bitmap | 4 MiB 块 | ≈0（bitmap ~200 KB） | +4–6 人日 | 重传 ≤ 4 MiB | 多流乱序写、块哈希表、接收端并发控制 |
| L3 rsync/zsync 增量 | 块 delta | 需旧文件读权限 | +5–8 人日 | 只传差异块（同目录更新才有意义） | 与「一次性发送新目录」目标不匹配 |
| 不做（现状） | 整目录 | 0 | 0 | 全部重传（25 GB ≈ 22 min） | 用户明确要求改进 |

---

## 7. 测试与验收（实现时执行）

1. **单元**：journal append/重放/坏行；manifest_digest 稳定性（路径排序、mtime 归一）；offset 边界（0、部分、=size、>size）。
2. **集成（loopback）**：25 GB 目录传到 10% / 50% / 99% 时 `taskkill /F` 发送端与接收端，各自重启后续传；断言最终文件树逐文件一致（路径|大小|xxh64）。
3. **故障注入**：`LANPIPE_FAULT_DROP_HASH` 仍须通过；journal 删除/截断一半；`.lp-part` 篡改（中间字节翻转）→ 最终 Verify 必须发现并重传。
4. **跨机（WSL）**：中断后续传，核对 43,594 文件/25,001,024,890 B；记录恢复耗时与重传字节数。
5. **回归**：现有 e2e / resync / 25GB 全量不退化；`cargo test` + `clippy -D warnings` 全绿。

验收标准：

- 中断后续传重传字节 ≤ `最后未完成文件 + 1 s 窗口数据`；
- 恢复后文件树与源 100% 一致（逐文件比对）；
- 无 `.lanpipe` 参与最终比对时，`.lp-part` 残留为 0；
- 首次发送（无 journal）零额外开销（manifest_digest 只哈希元数据）。

---

## 8. 落地顺序与工作量

| 阶段 | 内容 | 预估 |
| --- | --- | --- |
| R1 | journal 模块（写/读/压缩/清理）+ 单测 | 1 人日 |
| R2 | 协议帧 `ResumeQuery/ResumeState` + Offer 扩展 + 向后兼容（对端不支持则忽略） | 1 人日 |
| R3 | 接收端：journal 记录、partial 识别、跳过完成文件 | 1 人日 |
| R4 | 发送端：跳过/续写/进度校准 | 1 人日 |
| R5 | 崩溃与故障测试（kill -9、篡改、断网） | 1–2 人日 |
| R6 | 25 GB 跨机复测 + 文档/README | 0.5 人日 |

合计约 **5–6 人日**（不含 L2）。

---

## 9. 参考链接

- tus.io Resumable Upload Protocol 1.0.0 — <https://tus.io/protocols/resumable-upload>
- HTTP Semantics (Range requests), RFC 9110 — <https://www.rfc-editor.org/rfc/rfc9110>
- Tridgell & Mackerras, *The rsync algorithm*, TR-CS-96-05 — <https://rsync.samba.org/tech_report/>
- Tridgell, *Efficient Algorithms for Sorting and Synchronization*, PhD thesis, ANU 1999 — <https://www.samba.org/~tridge/phd_thesis.pdf>
- Xia et al., *FastCDC: a Fast and Efficient Content-Defined Chunking Approach for Data Deduplication*, USENIX ATC 2016 — <https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf>
- BitTorrent Protocol Specification, BEP-3 — <https://www.bittorrent.org/beps/bep_0003.html>
- Syncthing Block Exchange Protocol v1 — <https://docs.syncthing.net/specs/bep-v1.html>
- Syncthing, *Understanding Synchronization* — <https://docs.syncthing.net/users/syncing.html>
- iroh / iroh-blobs（BLAKE3 + bao 可验证流）— <https://www.iroh.computer/docs>、<https://docs.rs/iroh-blobs>
- zsync（HTTP 上的 rsync 式增量下载）— <http://zsync.moria.org.uk/>
- QUIC, RFC 9000 — <https://www.rfc-editor.org/rfc/rfc9000>；TLS 1.3 resumption, RFC 9001 — <https://www.rfc-editor.org/rfc/rfc9001>
- Microsoft robocopy（`/Z`、`/ZB` 断点续传）— <https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/robocopy>
- croc（断点续传/中继回退）— <https://github.com/schollz/croc>
