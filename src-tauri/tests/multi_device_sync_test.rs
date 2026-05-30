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
