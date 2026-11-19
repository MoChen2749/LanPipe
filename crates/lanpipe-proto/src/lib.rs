//! LP/1 协议：帧格式与消息结构。
//!
//! 帧头固定 12 字节：`magic(4) | type(1) | flags(1) | reserved(2) | len(4, big-endian)`，
//! 随后是 `len` 字节 payload。所有多字节整数均为大端。

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL_VERSION: u8 = 1;
pub const FRAME_MAGIC: [u8; 4] = *b"LPK1";
pub const HEADER_LEN: usize = 12;
pub const DEFAULT_UDP_PORT: u16 = 53310;
pub const DEFAULT_TCP_PORT: u16 = 53311;
pub const MULTICAST_GROUP: [u8; 4] = [239, 255, 42, 99];
pub const MAX_DATA_FRAME: u32 = 4 * 1024 * 1024;
/// 控制帧（Offer/Verify 等）上限：几万条目的清单可能到几 MB，DATA 帧仍单独限制。
pub const MAX_CONTROL_FRAME: u32 = 64 * 1024 * 1024;
pub const DATA_CHUNK: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    Hello = 1,
    Offer = 2,
    Accept = 3,
    Reject = 4,
    Data = 5,
    FileDone = 6,
    Done = 7,
    Error = 8,
    /// 接收端→发送端：逐文件哈希清单（用于校验与定位损坏文件）。
    Verify = 9,
    /// 发送端→接收端：要求重传的文件 id 列表。
    Resync = 10,
    /// 发送端→接收端：校验通过，传输完成。
    Complete = 11,
}

impl FrameType {
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            1 => Self::Hello,
            2 => Self::Offer,
            3 => Self::Accept,
            4 => Self::Reject,
            5 => Self::Data,
            6 => Self::FileDone,
            7 => Self::Done,
            8 => Self::Error,
            9 => Self::Verify,
            10 => Self::Resync,
            11 => Self::Complete,
            _ => return None,
        })
    }
}

/// 清单中的一项。`path` 是相对保存根目录的 POSIX 风格路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub id: u32,
    pub path: String,
    pub size: u64,
    pub mtime_ms: u64,
    #[serde(default)]
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub version: u8,
    pub session_id: String,
    pub root_name: String,
    pub entries: Vec<ManifestEntry>,
    pub total_bytes: u64,
    /// 清单摘要（xxHash64 十六进制），用于接收端查找断点续传 journal。
    #[serde(default)]
    pub manifest_digest: String,
}

/// 接收端已完成的文件（断点续传）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResumeState {
    pub completed: Vec<u32>,
    #[serde(default)]
    pub partial: Vec<FileOffset>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileOffset {
    pub id: u32,
    pub offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accept {
    pub session_id: String,
    /// 接收端找到可复用进度时返回；为空表示全量发送。
    #[serde(default)]
    pub resume: Option<ResumeState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reject {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub session_id: String,
    pub stream: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDone {
    pub id: u32,
}

/// 单个文件的校验结果（xxh64 流式哈希）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHash {
    pub id: u32,
    pub hash: u64,
}

/// 接收端回报的逐文件哈希清单。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerifyDigest {
    pub hashes: Vec<FileHash>,
}

/// 发送端要求重传的文件 id。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResyncIds {
    pub ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMsg {
    pub message: String,
}

/// 渲染 12 字节帧头。
pub fn encode_header(ty: FrameType, flags: u8, stream: u16, len: u32) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(&FRAME_MAGIC);
    h[4] = ty as u8;
    h[5] = flags;
    h[6..8].copy_from_slice(&stream.to_be_bytes());
    h[8..12].copy_from_slice(&len.to_be_bytes());
    h
}

pub struct FrameHeader {
    pub ty: FrameType,
    pub flags: u8,
    pub stream: u16,
    pub len: u32,
}

impl FrameHeader {
    pub fn decode(buf: &[u8; HEADER_LEN]) -> Result<Self, String> {
        if buf[0..4] != FRAME_MAGIC {
            return Err("不是 LP/1 帧（magic 不匹配）".into());
        }
        let ty = FrameType::from_u8(buf[4]).ok_or_else(|| format!("未知帧类型 {}", buf[4]))?;
        Ok(Self {
            ty,
            flags: buf[5],
            stream: u16::from_be_bytes([buf[6], buf[7]]),
            len: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
        })
    }
}

pub async fn write_frame<W>(
    w: &mut W,
    ty: FrameType,
    flags: u8,
    stream: u16,
    payload: &[u8],
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let header = encode_header(ty, flags, stream, payload.len() as u32);
    w.write_all(&header).await?;
    w.write_all(payload).await?;
    w.flush().await
}

/// 读取一帧。`max_len` 限制 payload 大小，防止恶意长度。
pub async fn read_frame<R>(
    r: &mut R,
    max_len: u32,
) -> std::io::Result<Option<(FrameType, u8, u16, Vec<u8>)>>
where
    R: AsyncRead + Unpin,
{
    let mut head = [0u8; HEADER_LEN];
    match r.read_exact(&mut head).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let h = FrameHeader::decode(&head).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if h.len > max_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("帧过大：{} > {}", h.len, max_len),
        ));
    }
    let mut payload = vec![0u8; h.len as usize];
    r.read_exact(&mut payload).await?;
    Ok(Some((h.ty, h.flags, h.stream, payload)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        let h = encode_header(FrameType::Data, 0, 7, 1234);
        let d = FrameHeader::decode(&h).unwrap();
        assert_eq!(d.ty, FrameType::Data);
        assert_eq!(d.stream, 7);
        assert_eq!(d.len, 1234);
    }

    #[test]
    fn bad_magic() {
        let mut h = encode_header(FrameType::Hello, 0, 0, 0);
        h[0] = b'X';
        assert!(FrameHeader::decode(&h).is_err());
    }

    #[tokio::test]
    async fn frame_roundtrip() {
        let (mut a, mut b) = tokio::io::duplex(64 * 1024);
        write_frame(&mut a, FrameType::Offer, 0, 0, b"{\"x\":1}").await.unwrap();
        let (ty, flags, stream, payload) = read_frame(&mut b, MAX_DATA_FRAME).await.unwrap().unwrap();
        assert_eq!(ty, FrameType::Offer);
        assert_eq!(flags, 0);
        assert_eq!(stream, 0);
        assert_eq!(payload, b"{\"x\":1}");
    }
}
