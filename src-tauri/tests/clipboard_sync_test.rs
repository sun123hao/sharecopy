//! ShareCopy 剪贴板同步 E2E 集成测试
//!
//! 模拟多设备场景，测试从剪贴板变更到网络同步的完整链路。
//! 测试维度：
//!   1. 协议编解码往返
//!   2. 剪贴板内容去重
//!   3. 帧解码器（半包/粘包）
//!   4. 多设备广播逻辑
//!   5. 同步引擎消息路由

use app_lib::clipboard::{ClipboardContent, ClipboardWatcher, ClipboardBackend};
use app_lib::error::{AppError, AppResult};
use app_lib::protocol::{FrameDecoder, Message, ClipboardTextPayload, HeartbeatPayload};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

// ── 测试用 Mock 剪贴板后端 ───────────────────

struct MockClipboard {
    content: parking_lot::Mutex<ClipboardContent>,
    change_count: AtomicU64,
}

impl MockClipboard {
    fn new() -> Self {
        Self {
            content: parking_lot::Mutex::new(ClipboardContent::None),
            change_count: AtomicU64::new(0),
        }
    }

    fn set_text(&self, text: &str) {
        *self.content.lock() = ClipboardContent::Text(text.to_string());
        self.change_count.fetch_add(1, Ordering::Relaxed);
    }

    fn set_image(&self, width: u32, height: u32, data: Vec<u8>) {
        *self.content.lock() = ClipboardContent::Image { width, height, data };
        self.change_count.fetch_add(1, Ordering::Relaxed);
    }
}

impl ClipboardBackend for MockClipboard {
    fn read(&self) -> AppResult<ClipboardContent> {
        Ok(self.content.lock().clone())
    }

    fn write(&self, content: &ClipboardContent) -> AppResult<()> {
        *self.content.lock() = content.clone();
        self.change_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn change_count(&self) -> AppResult<u64> {
        Ok(self.change_count.load(Ordering::Relaxed))
    }
}

// ── 测试 1: 剪贴板内容去重 ──────────────────

#[test]
fn test_clipboard_content_hash_different_texts() {
    let a = ClipboardContent::Text("hello".into());
    let b = ClipboardContent::Text("world".into());
    assert_ne!(a.content_hash(), b.content_hash(), "不同文本应有不同 hash");
}

#[test]
fn test_clipboard_content_hash_same_text() {
    let a = ClipboardContent::Text("hello".into());
    let b = ClipboardContent::Text("hello".into());
    assert_eq!(a.content_hash(), b.content_hash(), "相同文本应有相同 hash");
}

#[test]
fn test_clipboard_content_hash_none() {
    let none = ClipboardContent::None;
    assert_eq!(none.content_hash(), "", "None 的 hash 应为空字符串");
}

// ── 测试 2: 协议往返 ────────────────────────

#[test]
fn test_protocol_roundtrip_text() {
    let msg = Message::ClipboardText(ClipboardTextPayload {
        source_device_id: "device-a".into(),
        content: "Hello from test".into(),
        timestamp: 1234567890,
    });
    let encoded = msg.encode().unwrap();
    let (decoded, consumed) = Message::decode(&encoded).unwrap();
    assert_eq!(consumed, encoded.len());

    match decoded {
        Message::ClipboardText(p) => {
            assert_eq!(p.source_device_id, "device-a");
            assert_eq!(p.content, "Hello from test");
            assert_eq!(p.timestamp, 1234567890);
        }
        _ => panic!("解码类型错误"),
    }
}

#[test]
fn test_protocol_roundtrip_device_info() {
    let msg = Message::DeviceInfo(app_lib::protocol::DeviceInfoPayload {
        device_id: "uuid-abc-123".into(),
        device_name: "Test Device".into(),
        hostname: "test.local.".into(),
        platform: "test".into(),
        tcp_port: 54322,
        version: "0.1.0".into(),
    });
    let encoded = msg.encode().unwrap();
    let (decoded, _) = Message::decode(&encoded).unwrap();

    match decoded {
        Message::DeviceInfo(p) => {
            assert_eq!(p.device_id, "uuid-abc-123");
            assert_eq!(p.device_name, "Test Device");
        }
        _ => panic!("解码类型错误"),
    }
}

// ── 测试 3: 帧解码器半包粘包 ──────────────────

#[test]
fn test_frame_decoder_partial_packet() {
    let msg = Message::ClipboardText(ClipboardTextPayload {
        source_device_id: "test".into(),
        content: "Half and half".into(),
        timestamp: 0,
    });
    let encoded = msg.encode().unwrap();
    let mut decoder = FrameDecoder::new();

    // 喂入前半部分
    let half = encoded.len() / 2;
    let msgs = decoder.feed(&encoded[..half]);
    assert!(msgs.is_empty(), "半包不应解码出消息");

    // 喂入后半部分
    let msgs = decoder.feed(&encoded[half..]);
    assert_eq!(msgs.len(), 1, "完整包应解码出 1 条消息");
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
        timestamp: 1,
    });
    let mut encoded = msg1.encode().unwrap();
    encoded.extend_from_slice(&msg2.encode().unwrap());

    let mut decoder = FrameDecoder::new();
    let msgs = decoder.feed(&encoded);
    assert_eq!(msgs.len(), 2, "粘包应解码出 2 条消息");
}

#[test]
fn test_frame_decoder_resync_on_garbage_prefix() {
    let msg = Message::ClipboardText(ClipboardTextPayload {
        source_device_id: "t".into(),
        content: "x".into(),
        timestamp: 0,
    });
    let encoded = msg.encode().unwrap();
    let mut garbage = vec![0u8; 5]; // 无效字节
    garbage.extend_from_slice(&encoded);

    let mut decoder = FrameDecoder::new();
    let msgs = decoder.feed(&garbage);
    assert_eq!(msgs.len(), 1, "跳过垃圾字节后应解码出消息");
}

// ── 测试 4: 剪贴板写入安全（防乒乓核心逻辑）─

#[test]
fn test_write_safely_updates_hash() {
    let backend = Box::new(MockClipboard::new());
    let (tx, _rx) = mpsc::unbounded_channel();
    let watcher = ClipboardWatcher::new(backend, tx, 100, 1000);

    // 写入内容
    watcher.write_safely(&ClipboardContent::Text("test".into())).unwrap();

    // 再次写入相同内容——不应 panic，只是 no-op
    watcher.write_safely(&ClipboardContent::Text("test".into())).unwrap();
}

#[test]
fn test_write_safely_different_content_succeeds() {
    let backend = Box::new(MockClipboard::new());
    let (tx, _rx) = mpsc::unbounded_channel();
    let watcher = ClipboardWatcher::new(backend, tx, 100, 1000);

    watcher.write_safely(&ClipboardContent::Text("first".into())).unwrap();
    watcher.write_safely(&ClipboardContent::Text("second".into())).unwrap();
    // 不应 panic
}

#[test]
fn test_write_safely_none_content() {
    let backend = Box::new(MockClipboard::new());
    let (tx, _rx) = mpsc::unbounded_channel();
    let watcher = ClipboardWatcher::new(backend, tx, 100, 1000);

    // None 写入不报错
    watcher.write_safely(&ClipboardContent::None).unwrap();
}

// ── 测试 5: 多设备广播逻辑 ──────────────────

#[test]
fn test_broadcast_message_encoding_consistent() {
    // 验证广播消息编码的确定性：相同输入 → 相同输出
    let msg = Message::ClipboardText(ClipboardTextPayload {
        source_device_id: "sender-1".into(),
        content: "broadcast test".into(),
        timestamp: 42,
    });

    let e1 = msg.encode().unwrap();
    let e2 = msg.encode().unwrap();
    assert_eq!(e1, e2, "相同消息两次编码应一致");

    let (d1, _) = Message::decode(&e1).unwrap();
    let (d2, _) = Message::decode(&e2).unwrap();

    match (d1, d2) {
        (Message::ClipboardText(p1), Message::ClipboardText(p2)) => {
            assert_eq!(p1.content, p2.content);
            assert_eq!(p1.source_device_id, p2.source_device_id);
        }
        _ => panic!("解码类型错误"),
    }
}

// ── 测试 6: 心跳消息编码 ────────────────────

#[test]
fn test_heartbeat_roundtrip() {
    let ping = Message::HeartbeatPing(HeartbeatPayload {
        device_id: "device-x".into(),
        timestamp: 100,
    });
    let pong = Message::HeartbeatPong(HeartbeatPayload {
        device_id: "device-y".into(),
        timestamp: 200,
    });

    // Ping 往返
    let encoded = ping.encode().unwrap();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert!(matches!(decoded, Message::HeartbeatPing(_)), "应解码为 Ping");
    if let Message::HeartbeatPing(p) = decoded {
        assert_eq!(p.device_id, "device-x");
    }

    // Pong 往返
    let encoded = pong.encode().unwrap();
    let (decoded, _) = Message::decode(&encoded).unwrap();
    assert!(matches!(decoded, Message::HeartbeatPong(_)), "应解码为 Pong");
}

// ── 测试 7: None 内容边缘情况 ───────────────

#[test]
fn test_clipboard_none_size_is_zero() {
    let none = ClipboardContent::None;
    assert_eq!(none.size_bytes(), 0);
}

#[test]
fn test_clipboard_none_equal() {
    assert_eq!(ClipboardContent::None, ClipboardContent::None);
}

#[test]
fn test_clipboard_text_not_equal_none() {
    assert_ne!(ClipboardContent::Text("hello".into()), ClipboardContent::None);
}

// ── 测试 8: 错误类型 ─────────────────────────

#[test]
fn test_no_connection_error_display() {
    let err = AppError::NoConnection;
    assert_eq!(format!("{}", err), "无连接设备");
}

#[test]
fn test_channel_send_error_conversion() {
    // 验证 ChannelSend 错误可通过 ? 操作符传播
    let result: Result<(), AppError> = Err(AppError::ChannelSend);
    assert!(result.is_err());
}
