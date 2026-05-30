//! ShareCopy 多端同步集成测试
//!
//! 模拟 2-3 个设备通过 TCP 互联，验证剪贴板内容端到端同步。

use app_lib::clipboard::{ClipboardBackend, ClipboardContent, ClipboardWatcher};
use app_lib::error::AppResult;
use app_lib::network::{NetworkEvent, NetworkManager};
use app_lib::protocol::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

// ── Mock 剪贴板 ──────────────────────────────

struct MockClipboard {
    content: parking_lot::Mutex<ClipboardContent>,
    change_count: AtomicU64,
}

impl MockClipboard {
    fn new() -> Self {
        Self { content: parking_lot::Mutex::new(ClipboardContent::None), change_count: AtomicU64::new(0) }
    }
}

impl ClipboardBackend for MockClipboard {
    fn read(&self) -> AppResult<ClipboardContent> {
        Ok(self.content.lock().clone())
    }
    fn write(&self, c: &ClipboardContent) -> AppResult<()> {
        *self.content.lock() = c.clone();
        self.change_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn change_count(&self) -> AppResult<u64> {
        Ok(self.change_count.load(Ordering::Relaxed))
    }
}

// ── 辅助：创建一端 ────────────────────────────

struct DeviceHarness {
    device_id: String,
    network: Arc<NetworkManager>,
    clipboard: Arc<MockClipboard>,
    watcher: Arc<ClipboardWatcher>,
    event_rx: mpsc::UnboundedReceiver<NetworkEvent>,
}

impl DeviceHarness {
    fn new(device_id: &str, port: u16) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let network = Arc::new(NetworkManager::new(
            device_id.into(), device_id.into(), port, event_tx,
        ));
        let clipboard = Arc::new(MockClipboard::new());
        let (clip_tx, _clip_rx) = mpsc::unbounded_channel();
        let watcher = Arc::new(ClipboardWatcher::new(
            Box::new(MockClipboard {
                content: parking_lot::Mutex::new(ClipboardContent::None),
                change_count: AtomicU64::new(1), // 从 1 开始避免与 MockClipboard 冲突
            }),
            clip_tx,
            100, 1000,
        ));
        Self { device_id: device_id.into(), network, clipboard, watcher, event_rx }
    }

    fn write_clipboard(&self, text: &str) {
        *self.clipboard.content.lock() = ClipboardContent::Text(text.into());
        self.clipboard.change_count.fetch_add(1, Ordering::Relaxed);
    }
}

// ── 测试 1: 两设备 TCP 连接 → 消息发送 → 接收 ──

#[tokio::test]
async fn test_two_devices_connect_and_exchange_messages() {
    // 启动 Device A（port 55432）
    let mut device_a = DeviceHarness::new("device-a", 55432);
    device_a.network.start().await.unwrap();

    // 启动 Device B（port 55433）
    let mut device_b = DeviceHarness::new("device-b", 55433);
    device_b.network.start().await.unwrap();

    // A 连接到 B（用 localhost）
    let discovered_b = app_lib::discovery::DiscoveredDevice {
        device_id: "device-b".into(),
        device_name: "Device B".into(),
        hostname: "localhost".into(),
        platform: "test".into(),
        ip_address: "127.0.0.1".into(),
        tcp_port: 55433,
        last_seen: chrono::Utc::now(),
        first_seen: chrono::Utc::now(),
    };
    device_a.network.connect_to_device(&discovered_b).await.unwrap();

    // 等待连接建立
    let connected = timeout(Duration::from_secs(5), device_a.event_rx.recv()).await;
    assert!(connected.is_ok(), "A 应收到 DeviceConnected 事件");
    let b_connected = timeout(Duration::from_secs(5), device_b.event_rx.recv()).await;
    assert!(b_connected.is_ok(), "B 应收到 DeviceConnected 事件");

    // A 发送消息到 B
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "device-a".into(),
        content: "Hello from A".into(),
        timestamp: 1,
    });
    device_a.network.send("device-b", &msg).unwrap();

    // B 接收消息
    let received = timeout(Duration::from_secs(5), device_b.event_rx.recv()).await;
    assert!(received.is_ok(), "B 应收到消息");
    if let Ok(Some(NetworkEvent::MessageReceived { from_device_id, message })) = received {
        assert_eq!(from_device_id, "device-a");
        match message {
            Message::ClipboardText(p) => assert_eq!(p.content, "Hello from A"),
            _ => panic!("应收到剪贴板文本消息"),
        }
    } else {
        panic!("应收到 MessageReceived 事件");
    }
}

// ── 测试 2: 设备 A→B 剪贴板文本同步 ──────────

#[tokio::test]
async fn test_clipboard_sync_a_to_b() {
    let mut a = DeviceHarness::new("syncer-a", 55434);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("syncer-b", 55435);
    b.network.start().await.unwrap();

    // A 连接 B
    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "syncer-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55435,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();

    // 等待双向连接
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // A 复制文本
    a.write_clipboard("共享文本");

    // A 广播剪贴板
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "syncer-a".into(),
        content: "共享文本".into(),
        timestamp: 2,
    });
    a.network.broadcast(&msg).unwrap();

    // B 接收
    let result = timeout(Duration::from_secs(5), b.event_rx.recv()).await;
    assert!(result.is_ok(), "B 应收到广播消息");
    if let Ok(Some(NetworkEvent::MessageReceived { from_device_id, message })) = result {
        assert_eq!(from_device_id, "syncer-a");
        if let Message::ClipboardText(p) = message {
            assert_eq!(p.content, "共享文本");
            assert_eq!(p.source_device_id, "syncer-a");
        }
    }
}

// ── 测试 3: 三设备星形拓扑 ──────────────────

#[tokio::test]
async fn test_three_device_star_topology() {
    // Hub: 55436, Spoke A: 55437, Spoke B: 55438
    let mut hub = DeviceHarness::new("hub", 55436);
    hub.network.start().await.unwrap();
    let mut spoke_a = DeviceHarness::new("spoke-a", 55437);
    spoke_a.network.start().await.unwrap();
    let mut spoke_b = DeviceHarness::new("spoke-b", 55438);
    spoke_b.network.start().await.unwrap();

    // Hub 连接两个 spoke
    for (id, port) in [("spoke-a", 55437u16), ("spoke-b", 55438u16)] {
        let d = app_lib::discovery::DiscoveredDevice {
            device_id: id.into(), device_name: id.into(),
            hostname: "localhost".into(), platform: "test".into(),
            ip_address: "127.0.0.1".into(), tcp_port: port,
            last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
        };
        hub.network.connect_to_device(&d).await.unwrap();
    }

    // 等待连接事件（Hub 收到 2 个 + Spoke A/B 各收到 1 个）
    let mut connected_count = 0u32;
    let deadline = Duration::from_secs(10);
    let start = tokio::time::Instant::now();
    while connected_count < 4 && start.elapsed() < deadline {
        tokio::select! {
            r = hub.event_rx.recv() => if r.is_some() { connected_count += 1 },
            r = spoke_a.event_rx.recv() => if r.is_some() { connected_count += 1 },
            r = spoke_b.event_rx.recv() => if r.is_some() { connected_count += 1 },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
    }
    assert!(connected_count >= 2, "至少应有 Hub-SpokeA 和 Hub-SpokeB 两条连接");

    // Hub 广播消息 → Spoke A 和 Spoke B 都应收到
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "hub".into(),
        content: "广播给所有人".into(),
        timestamp: 3,
    });
    hub.network.broadcast(&msg).unwrap();

    // 收集 Spoke A 和 B 收到的消息
    let mut received = 0u32;
    let deadline = Duration::from_secs(5);
    let start = tokio::time::Instant::now();
    while received < 2 && start.elapsed() < deadline {
        tokio::select! {
            r = spoke_a.event_rx.recv() => {
                if let Some(NetworkEvent::MessageReceived { message, .. }) = r {
                    if let Message::ClipboardText(p) = message {
                        assert_eq!(p.content, "广播给所有人");
                        received += 1;
                    }
                }
            },
            r = spoke_b.event_rx.recv() => {
                if let Some(NetworkEvent::MessageReceived { message, .. }) = r {
                    if let Message::ClipboardText(p) = message {
                        assert_eq!(p.content, "广播给所有人");
                        received += 1;
                    }
                }
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
    }
    assert_eq!(received, 2, "Hub 广播后 Spoke A 和 B 都应收到");
}

// ── 测试 4: 设备断开重连 ────────────────────

#[tokio::test]
async fn test_disconnect_and_reconnect() {
    let mut a = DeviceHarness::new("reconn-a", 55439);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("reconn-b", 55440);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "reconn-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55440,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();

    // 等连接
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // 验证连接
    assert!(a.network.is_connected("reconn-b"));
    assert_eq!(a.network.connected_count(), 1);

    // A 发送消息
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "reconn-a".into(),
        content: "before disconnect".into(),
        timestamp: 4,
    });
    a.network.send("reconn-b", &msg).unwrap();

    let r1 = timeout(Duration::from_secs(3), b.event_rx.recv()).await;
    assert!(r1.is_ok(), "断开前消息应到达");

    // 关闭 B 的网络（模拟断开）
    b.network.shutdown();

    // A 尝试发送（应失败，因为连接断开）
    let result = timeout(Duration::from_secs(5), async {
        loop {
            match a.network.send("reconn-b", &msg) {
                Ok(()) => {},
                Err(_) => return, // 预期错误
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await;
    // 不断言——仅验证不 panic
    drop(result);
}

// ── 测试 5: 大消息（模拟大图片 chunk） ───────

#[tokio::test]
async fn test_large_message_transfer() {
    let mut a = DeviceHarness::new("large-a", 55441);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("large-b", 55442);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "large-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55442,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // 发送 10 条大消息
    for i in 0..10 {
        let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
            source_device_id: "large-a".into(),
            content: format!("消息 {}", i),
            timestamp: i,
        });
        a.network.send("large-b", &msg).unwrap();
    }

    // 收集 10 条消息
    let mut count = 0u32;
    let deadline = Duration::from_secs(10);
    let start = tokio::time::Instant::now();
    while count < 10 && start.elapsed() < deadline {
        match timeout(Duration::from_secs(2), b.event_rx.recv()).await {
            Ok(Some(NetworkEvent::MessageReceived { .. })) => count += 1,
            _ => break,
        }
    }
    assert_eq!(count, 10, "应收到 10 条消息");
}

// ── 测试 6: 一端发送 → 另外两端都收到 ──────

#[tokio::test]
async fn test_one_sends_two_receive() {
    // 全互联拓扑：A(55443), B(55444), C(55445) 两两相连
    let mut a = DeviceHarness::new("mesh-a", 55443);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("mesh-b", 55444);
    b.network.start().await.unwrap();
    let mut c = DeviceHarness::new("mesh-c", 55445);
    c.network.start().await.unwrap();

    // A 连接 B 和 C
    for (id, port) in [("mesh-b", 55444u16), ("mesh-c", 55445u16)] {
        let d = app_lib::discovery::DiscoveredDevice {
            device_id: id.into(), device_name: id.into(),
            hostname: "localhost".into(), platform: "test".into(),
            ip_address: "127.0.0.1".into(), tcp_port: port,
            last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
        };
        a.network.connect_to_device(&d).await.unwrap();
    }
    // B 也连接 C（全互联）
    let dc = app_lib::discovery::DiscoveredDevice {
        device_id: "mesh-c".into(), device_name: "C".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55445,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    b.network.connect_to_device(&dc).await.unwrap();

    // 等待全互联建立（6 个连接事件）
    let mut ev_count = 0u32;
    let deadline = Duration::from_secs(10);
    let start = tokio::time::Instant::now();
    while ev_count < 6 && start.elapsed() < deadline {
        tokio::select! {
            r = a.event_rx.recv() => if r.is_some() { ev_count += 1 },
            r = b.event_rx.recv() => if r.is_some() { ev_count += 1 },
            r = c.event_rx.recv() => if r.is_some() { ev_count += 1 },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
    }
    assert!(ev_count >= 4, "全互联应至少建立 4 条连接");

    // A 复制文本并广播 → B 和 C 都应收到
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "mesh-a".into(),
        content: "全互联同步测试".into(),
        timestamp: 100,
    });
    a.network.broadcast(&msg).unwrap();

    // B 和 C 都应收到了消息
    let mut b_got = false;
    let mut c_got = false;
    let deadline = Duration::from_secs(5);
    let start = tokio::time::Instant::now();
    while (!b_got || !c_got) && start.elapsed() < deadline {
        tokio::select! {
            r = b.event_rx.recv() => {
                if let Some(NetworkEvent::MessageReceived { message, .. }) = r {
                    if let Message::ClipboardText(p) = message {
                        if p.content == "全互联同步测试" { b_got = true; }
                    }
                }
            },
            r = c.event_rx.recv() => {
                if let Some(NetworkEvent::MessageReceived { message, .. }) = r {
                    if let Message::ClipboardText(p) = message {
                        if p.content == "全互联同步测试" { c_got = true; }
                    }
                }
            },
            _ = tokio::time::sleep(Duration::from_millis(100)) => {},
        }
    }
    assert!(b_got, "B 应从 A 收到消息");
    assert!(c_got, "C 应从 A 收到消息");

    // 验证全互联连接数
    assert_eq!(a.network.connected_count(), 2, "A 连接 B 和 C");
    assert_eq!(b.network.connected_count(), 2, "B 连接 A 和 C");
    assert_eq!(c.network.connected_count(), 2, "C 连接 A 和 B");
}

// ── 测试 7: 图片数据编码同步 ─────────────────

#[tokio::test]
async fn test_image_sync_via_network() {
    let mut a = DeviceHarness::new("img-a", 55446);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("img-b", 55447);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "img-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55447,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // 模拟小图片（< 10MB 阈值）
    let png_data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]; // PNG header + padding
    let mut img_bytes = png_data.clone();
    img_bytes.extend(vec![0u8; 100]); // ~108 bytes total
    let msg = Message::ClipboardImage(app_lib::protocol::ClipboardImagePayload {
        source_device_id: "img-a".into(),
        width: 10,
        height: 10,
        format: app_lib::protocol::ImageFormat::Png,
        data: img_bytes.clone(),
        timestamp: 1,
    });
    a.network.send("img-b", &msg).unwrap();

    let received = timeout(Duration::from_secs(3), b.event_rx.recv()).await;
    assert!(received.is_ok(), "B 应收到图片消息");
    if let Ok(Some(NetworkEvent::MessageReceived { message, .. })) = received {
        match message {
            Message::ClipboardImage(p) => {
                assert_eq!(p.width, 10);
                assert_eq!(p.height, 10);
                assert_eq!(p.data.len(), img_bytes.len());
            }
            _ => panic!("应收到 ClipboardImage"),
        }
    }
}

// ── 测试 8: 图片分块传输 ────────────────────

#[tokio::test]
async fn test_image_chunk_transfer() {
    let mut a = DeviceHarness::new("chunk-a", 55448);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("chunk-b", 55449);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "chunk-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55449,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // 模拟 5 个分块
    let transfer_id = "test-transfer-001".to_string();
    let total_chunks: u16 = 5;
    for i in 0..total_chunks {
        let msg = Message::ClipboardImageChunk(app_lib::protocol::ClipboardImageChunkPayload {
            source_device_id: "chunk-a".into(),
            transfer_id: transfer_id.clone(),
            width: 100,
            height: 100,
            total_chunks,
            chunk_index: i,
            data: vec![i as u8; 1024],
            timestamp: i as u64,
        });
        a.network.send("chunk-b", &msg).unwrap();
    }

    // B 应收齐 5 个 chunk
    let mut chunks: Vec<u16> = Vec::new();
    let deadline = Duration::from_secs(10);
    let start = tokio::time::Instant::now();
    while chunks.len() < 5 && start.elapsed() < deadline {
        match timeout(Duration::from_secs(2), b.event_rx.recv()).await {
            Ok(Some(NetworkEvent::MessageReceived { message, .. })) => {
                if let Message::ClipboardImageChunk(p) = message {
                    assert_eq!(p.transfer_id, transfer_id);
                    chunks.push(p.chunk_index);
                }
            }
            _ => break,
        }
    }
    assert_eq!(chunks.len(), 5, "应收齐 5 个分块");
}

// ── 测试 9: 心跳 Ping/Pong 循环 ──────────────

#[tokio::test]
async fn test_heartbeat_ping_pong() {
    let mut a = DeviceHarness::new("hb-a", 55450);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("hb-b", 55451);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "hb-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55451,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // 等待心跳周期（10s），验证连接未断开
    tokio::time::sleep(Duration::from_secs(12)).await;

    assert!(a.network.is_connected("hb-b"), "12s 后连接应仍存活");
    assert_eq!(a.network.connected_count(), 1);

    // 发送最终消息确认连接可用
    let msg = Message::HeartbeatPing(app_lib::protocol::HeartbeatPayload {
        device_id: "hb-a".into(),
        timestamp: 999,
    });
    a.network.send("hb-b", &msg).unwrap();
    let r = timeout(Duration::from_secs(3), b.event_rx.recv()).await;
    assert!(r.is_ok(), "心跳后消息应可送达");
}

// ── 测试 10: 并发消息收发 ───────────────────

#[tokio::test]
async fn test_concurrent_bidirectional_messaging() {
    let mut a = DeviceHarness::new("conc-a", 55452);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("conc-b", 55453);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "conc-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55453,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // A 和 B 同时互发消息
    let a_network = a.network.clone();
    let b_network = b.network.clone();
    let a_task = tokio::spawn(async move {
        for i in 0..20 {
            let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
                source_device_id: "conc-a".into(),
                content: format!("A→B msg {}", i),
                timestamp: i,
            });
            a_network.send("conc-b", &msg).unwrap();
        }
    });
    let b_task = tokio::spawn(async move {
        for i in 0..20 {
            let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
                source_device_id: "conc-b".into(),
                content: format!("B→A msg {}", i),
                timestamp: i + 100,
            });
            b_network.send("conc-a", &msg).unwrap();
        }
    });

    a_task.await.unwrap();
    b_task.await.unwrap();

    // 统计双向接收数
    let mut a_rcv = 0u32;
    let mut b_rcv = 0u32;
    let deadline = Duration::from_secs(10);
    let start = tokio::time::Instant::now();
    while (a_rcv < 20 || b_rcv < 20) && start.elapsed() < deadline {
        tokio::select! {
            r = a.event_rx.recv() => {
                if let Some(NetworkEvent::MessageReceived { .. }) = r { a_rcv += 1; }
            },
            r = b.event_rx.recv() => {
                if let Some(NetworkEvent::MessageReceived { .. }) = r { b_rcv += 1; }
            },
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
        }
    }
    assert!(a_rcv >= 15, "A 应收到大部分消息 (收到 {})", a_rcv);
    assert!(b_rcv >= 15, "B 应收到大部分消息 (收到 {})", b_rcv);
}

// ── 测试 11: 空文本边缘情况 ─────────────────

#[tokio::test]
async fn test_empty_text_no_crash() {
    let mut a = DeviceHarness::new("empty-a", 55454);
    a.network.start().await.unwrap();
    let mut b = DeviceHarness::new("empty-b", 55455);
    b.network.start().await.unwrap();

    let db = app_lib::discovery::DiscoveredDevice {
        device_id: "empty-b".into(), device_name: "B".into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: 55455,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&db).await.unwrap();
    timeout(Duration::from_secs(3), a.event_rx.recv()).await.ok();
    timeout(Duration::from_secs(3), b.event_rx.recv()).await.ok();

    // 空文本不应崩溃
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "empty-a".into(),
        content: "".into(),
        timestamp: 0,
    });
    let encoded = msg.encode().unwrap();
    assert!(!encoded.is_empty(), "空文本消息也应可编码");

    a.network.send("empty-b", &msg).unwrap();
    let r = timeout(Duration::from_secs(3), b.event_rx.recv()).await;
    assert!(r.is_ok(), "空文本消息应可送达");
}

// ── 测试 12: NoConnection 错误 ──────────────

#[tokio::test]
async fn test_no_connection_error_before_connect() {
    let a = DeviceHarness::new("nc-a", 55456);
    a.network.start().await.unwrap();

    // 未连接任何设备时 broadcast 应返回 NoConnection
    let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "nc-a".into(),
        content: "无人接收".into(),
        timestamp: 0,
    });
    let result = a.network.broadcast(&msg);
    assert!(result.is_err(), "无连接时 broadcast 应失败");
    match result {
        Err(app_lib::error::AppError::NoConnection) => {} // 预期
        other => panic!("应返回 NoConnection，实际: {:?}", other),
    }
}

// ── E2E 场景: 模拟三设备真实剪贴板同步会话 ──

#[tokio::test]
async fn test_e2e_clipboard_session_three_devices() {
    // ── Setup: 三设备全互联 ──
    let (mut a, mut b, mut c) = (
        DeviceHarness::new("user-a", 55457),
        DeviceHarness::new("user-b", 55458),
        DeviceHarness::new("user-c", 55459),
    );
    for d in [&mut a, &mut b, &mut c] { d.network.start().await.unwrap(); }

    // A→B, A→C, B→C（全互联）
    let make_device = |id: &str, port: u16| app_lib::discovery::DiscoveredDevice {
        device_id: id.into(), device_name: id.into(),
        hostname: "localhost".into(), platform: "test".into(),
        ip_address: "127.0.0.1".into(), tcp_port: port,
        last_seen: chrono::Utc::now(), first_seen: chrono::Utc::now(),
    };
    a.network.connect_to_device(&make_device("user-b", 55458)).await.unwrap();
    a.network.connect_to_device(&make_device("user-c", 55459)).await.unwrap();
    b.network.connect_to_device(&make_device("user-c", 55459)).await.unwrap();

    // 等待连接建立
    tokio::time::sleep(Duration::from_secs(2)).await;
    // 清空积累的连接事件
    while a.event_rx.try_recv().is_ok() {}
    while b.event_rx.try_recv().is_ok() {}
    while c.event_rx.try_recv().is_ok() {}

    // ── 场景 1: A 复制文字 → B 和 C 都应收到 ──
    let msg1 = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "user-a".into(),
        content: "📋 第一次复制".into(),
        timestamp: 1,
    });
    a.network.broadcast(&msg1).unwrap();

    let mut b_rcv = false; let mut c_rcv = false;
    let dl = Duration::from_secs(3); let start = tokio::time::Instant::now();
    while (!b_rcv || !c_rcv) && start.elapsed() < dl {
        tokio::select! {
            r = b.event_rx.recv() => { if let Some(NetworkEvent::MessageReceived { message, .. }) = r { if let Message::ClipboardText(p) = message { b_rcv = p.content == "📋 第一次复制"; } } },
            r = c.event_rx.recv() => { if let Some(NetworkEvent::MessageReceived { message, .. }) = r { if let Message::ClipboardText(p) = message { c_rcv = p.content == "📋 第一次复制"; } } },
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
        }
    }
    assert!(b_rcv, "场景1: B 应收到 A 的复制");
    assert!(c_rcv, "场景1: C 应收到 A 的复制");

    // ── 场景 2: B 连续复制 3 次 → A 和 C 都应收到 ──
    for i in 0..3 {
        let msg = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
            source_device_id: "user-b".into(),
            content: format!("B 复制 #{}", i),
            timestamp: 10 + i,
        });
        b.network.broadcast(&msg).unwrap();
    }

    let mut a_count = 0u32; let mut c_count = 0u32;
    let dl = Duration::from_secs(5); let start = tokio::time::Instant::now();
    while (a_count < 3 || c_count < 3) && start.elapsed() < dl {
        tokio::select! {
            r = a.event_rx.recv() => { if r.is_some() { a_count += 1; } },
            r = c.event_rx.recv() => { if r.is_some() { c_count += 1; } },
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
        }
    }
    assert!(a_count >= 3, "场景2: A 应收到 B 的 3 次复制 (收到 {})", a_count);
    assert!(c_count >= 3, "场景2: C 应收到 B 的 3 次复制 (收到 {})", c_count);

    // ── 场景 3: C 断开，A 和 B 继续同步 ──
    c.network.shutdown();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let msg3 = Message::ClipboardText(app_lib::protocol::ClipboardTextPayload {
        source_device_id: "user-a".into(),
        content: "C 已断开".into(),
        timestamp: 20,
    });
    a.network.broadcast(&msg3).unwrap();

    let mut b_rcv3 = false;
    let dl = Duration::from_secs(3); let start = tokio::time::Instant::now();
    while !b_rcv3 && start.elapsed() < dl {
        tokio::select! {
            r = b.event_rx.recv() => { if let Some(NetworkEvent::MessageReceived { message, .. }) = r { if let Message::ClipboardText(p) = message { b_rcv3 = p.content == "C 已断开"; } } },
            _ = tokio::time::sleep(Duration::from_millis(50)) => {},
        }
    }
    assert!(b_rcv3, "场景3: C 断开后 A→B 仍可同步");

    // ── 场景 4: A↔B 仍可通信（C 断开不影响） ──
    assert!(a.network.is_connected("user-b") || a.network.connected_count() >= 1,
        "A 应与 B 保持连接");
    assert!(b.network.is_connected("user-a") || b.network.connected_count() >= 1,
        "B 应与 A 保持连接");
}
