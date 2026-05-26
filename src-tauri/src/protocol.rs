use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

// ── 协议常量 ─────────────────────────────────
pub const MAGIC: u32 = 0x53484350; // "SHCP"
pub const PROTOCOL_VERSION: u8 = 1;
pub const HEADER_SIZE: usize = 11; // 4 + 1 + 2 + 4

// ── 消息类型枚举 ──────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum MessageType {
    ClipboardText = 0x0001,
    ClipboardImage = 0x0002,
    FileTransferReq = 0x0003,
    FileDataChunk = 0x0004,
    HeartbeatPing = 0x0005,
    HeartbeatPong = 0x0006,
    DeviceInfo = 0x0007,
    ClipboardImageChunk = 0x0008,
    Ack = 0x0009,
    Error = 0x000A,
}

impl MessageType {
    pub fn from_u16(v: u16) -> Option<Self> {
        match v {
            0x0001 => Some(Self::ClipboardText),
            0x0002 => Some(Self::ClipboardImage),
            0x0003 => Some(Self::FileTransferReq),
            0x0004 => Some(Self::FileDataChunk),
            0x0005 => Some(Self::HeartbeatPing),
            0x0006 => Some(Self::HeartbeatPong),
            0x0007 => Some(Self::DeviceInfo),
            0x0008 => Some(Self::ClipboardImageChunk),
            0x0009 => Some(Self::Ack),
            0x000A => Some(Self::Error),
            _ => None,
        }
    }
}

// ── 消息体枚举 ──────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    ClipboardText(ClipboardTextPayload),
    ClipboardImage(ClipboardImagePayload),
    FileTransferReq(FileTransferReqPayload),
    FileDataChunk(FileDataChunkPayload),
    HeartbeatPing(HeartbeatPayload),
    HeartbeatPong(HeartbeatPayload),
    DeviceInfo(DeviceInfoPayload),
    ClipboardImageChunk(ClipboardImageChunkPayload),
    Ack(AckPayload),
    Error(ErrorPayload),
}

// ── 各消息体定义 ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfoPayload {
    pub device_id: String,
    pub device_name: String,
    pub hostname: String,
    pub platform: String,
    pub tcp_port: u16,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardTextPayload {
    pub source_device_id: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardImagePayload {
    pub source_device_id: String,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageFormat {
    Png = 0x01,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardImageChunkPayload {
    pub source_device_id: String,
    pub transfer_id: String,
    pub width: u32,
    pub height: u32,
    pub total_chunks: u16,
    pub chunk_index: u16,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTransferReqPayload {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub total_chunks: u32,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDataChunkPayload {
    pub transfer_id: String,
    pub chunk_index: u32,
    pub total_chunks: u32,
    pub data: Vec<u8>,
    pub sha256_chunk: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    pub device_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckPayload {
    pub transfer_id: String,
    pub chunk_index: Option<u32>,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPayload {
    pub code: u16,
    pub message: String,
}

// ── 编码接口 ──────────────────────────────
impl Message {
    /// 获取消息类型
    pub fn message_type(&self) -> MessageType {
        match self {
            Message::ClipboardText(_) => MessageType::ClipboardText,
            Message::ClipboardImage(_) => MessageType::ClipboardImage,
            Message::FileTransferReq(_) => MessageType::FileTransferReq,
            Message::FileDataChunk(_) => MessageType::FileDataChunk,
            Message::HeartbeatPing(_) => MessageType::HeartbeatPing,
            Message::HeartbeatPong(_) => MessageType::HeartbeatPong,
            Message::DeviceInfo(_) => MessageType::DeviceInfo,
            Message::ClipboardImageChunk(_) => MessageType::ClipboardImageChunk,
            Message::Ack(_) => MessageType::Ack,
            Message::Error(_) => MessageType::Error,
        }
    }

    /// 将消息编码为完整的 TLV 字节帧
    pub fn encode(&self) -> AppResult<Vec<u8>> {
        let payload = bincode::serialize(self)?;
        let total_len = HEADER_SIZE + payload.len();
        let mut buf = Vec::with_capacity(total_len);

        // Magic (4 bytes, 小端序)
        buf.extend_from_slice(&MAGIC.to_le_bytes());
        // Version (1 byte)
        buf.push(PROTOCOL_VERSION);
        // Type (2 bytes, 小端序)
        buf.extend_from_slice(&(self.message_type() as u16).to_le_bytes());
        // Length (4 bytes, 小端序)
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        // Payload
        buf.extend_from_slice(&payload);

        Ok(buf)
    }

    /// 从字节流解码为 Message
    pub fn decode(buf: &[u8]) -> AppResult<(Self, usize)> {
        if buf.len() < HEADER_SIZE {
            return Err(crate::error::AppError::Protocol("数据不足".into()));
        }

        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC {
            return Err(crate::error::AppError::Protocol(format!(
                "无效魔数: 0x{:08X}",
                magic
            )));
        }

        let _version = buf[4];
        let msg_type =
            MessageType::from_u16(u16::from_le_bytes([buf[5], buf[6]])).ok_or_else(|| {
                crate::error::AppError::Protocol(format!("未知消息类型: {}", u16::from_le_bytes([buf[5], buf[6]])))
            })?;

        let payload_len = u32::from_le_bytes([buf[7], buf[8], buf[9], buf[10]]) as usize;

        if buf.len() < HEADER_SIZE + payload_len {
            return Err(crate::error::AppError::Protocol(format!(
                "负载不足: 需要 {}, 实际 {}",
                payload_len,
                buf.len() - HEADER_SIZE
            )));
        }

        let payload = &buf[HEADER_SIZE..HEADER_SIZE + payload_len];
        let message: Message = bincode::deserialize(payload)?;
        let consumed = HEADER_SIZE + payload_len;

        Ok((message, consumed))
    }
}

// ── 帧解码器（流式，处理 TCP 半包/粘包）──────
pub struct FrameDecoder {
    buffer: BytesMut,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::new(),
        }
    }

    /// 喂入新收到的字节，尝试解码出完整帧
    pub fn feed(&mut self, data: &[u8]) -> Vec<Message> {
        self.buffer.extend_from_slice(data);
        let mut messages = Vec::new();

        loop {
            if self.buffer.len() < HEADER_SIZE {
                break;
            }

            // 检查 magic
            let magic_bytes = &self.buffer[0..4];
            let magic = u32::from_le_bytes([magic_bytes[0], magic_bytes[1], magic_bytes[2], magic_bytes[3]]);
            if magic != MAGIC {
                tracing::warn!("帧同步丢失，魔数不匹配: 0x{:08X}", magic);
                // 移除无效字节，尝试重新同步
                self.buffer.advance(1);
                continue;
            }

            let payload_len =
                u32::from_le_bytes([
                    self.buffer[7],
                    self.buffer[8],
                    self.buffer[9],
                    self.buffer[10],
                ]) as usize;

            let total_frame_len = HEADER_SIZE + payload_len;
            if self.buffer.len() < total_frame_len {
                // 半包，等待更多数据
                break;
            }

            // 解码完整帧
            let frame_data = self.buffer[..total_frame_len].to_vec();
            match Message::decode(&frame_data) {
                Ok((msg, _)) => {
                    messages.push(msg);
                    self.buffer.advance(total_frame_len);
                }
                Err(e) => {
                    tracing::error!("帧解码失败: {}", e);
                    self.buffer.advance(1); // 跳过一字节尝试重新同步
                }
            }
        }

        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip_text() {
        let msg = Message::ClipboardText(ClipboardTextPayload {
            source_device_id: "test-device".into(),
            content: "Hello World".into(),
            timestamp: 1234567890,
        });

        let encoded = msg.encode().unwrap();
        let (decoded, consumed) = Message::decode(&encoded).unwrap();

        assert_eq!(consumed, encoded.len());

        match decoded {
            Message::ClipboardText(p) => {
                assert_eq!(p.source_device_id, "test-device");
                assert_eq!(p.content, "Hello World");
                assert_eq!(p.timestamp, 1234567890);
            }
            _ => panic!("解码类型不匹配"),
        }
    }

    #[test]
    fn test_frame_decoder_partial_packet() {
        let msg = Message::ClipboardText(ClipboardTextPayload {
            source_device_id: "test".into(),
            content: "Hello".into(),
            timestamp: 0,
        });
        let encoded = msg.encode().unwrap();

        let mut decoder = FrameDecoder::new();
        // 只喂入一半
        let half = encoded.len() / 2;
        let messages = decoder.feed(&encoded[..half]);
        assert!(messages.is_empty());

        // 喂入另一半
        let messages = decoder.feed(&encoded[half..]);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_frame_decoder_multiple_packets() {
        let msg1 = Message::ClipboardText(ClipboardTextPayload {
            source_device_id: "a".into(),
            content: "first".into(),
            timestamp: 0,
        });
        let msg2 = Message::ClipboardText(ClipboardTextPayload {
            source_device_id: "b".into(),
            content: "second".into(),
            timestamp: 0,
        });

        let mut encoded = msg1.encode().unwrap();
        encoded.extend_from_slice(&msg2.encode().unwrap());

        let mut decoder = FrameDecoder::new();
        let messages = decoder.feed(&encoded);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_encode_decode_device_info() {
        let msg = Message::DeviceInfo(DeviceInfoPayload {
            device_id: "uuid-123".into(),
            device_name: "MyMac".into(),
            hostname: "mac.local".into(),
            platform: "macos".into(),
            tcp_port: 54322,
            version: "0.1.0".into(),
        });

        let encoded = msg.encode().unwrap();
        let (decoded, _) = Message::decode(&encoded).unwrap();

        match decoded {
            Message::DeviceInfo(p) => {
                assert_eq!(p.device_id, "uuid-123");
                assert_eq!(p.device_name, "MyMac");
                assert_eq!(p.platform, "macos");
            }
            _ => panic!("解码类型不匹配"),
        }
    }
}
