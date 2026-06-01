# ShareCopy

> 局域网剪贴板共享 & 文件传输工具 —— 无需联网、无需注册、无需服务器

跨平台实时同步剪贴板（文字/图片），支持点对点文件传输。同一局域网内多台设备自动发现、直连通信，数据不上传任何服务器。

---

## 功能

- **实时剪贴板同步** — 一处复制，多端粘贴。文字、图片双向实时同步
- **点对点文件传输** — 点击发送或拖放文件到设备卡片，局域网高速直传
- **剪贴板历史** — 保留同步记录，支持搜索、重新复制、自定义保留天数
- **设备自动发现** — 基于 mDNS 自动发现局域网内其他设备，零配置
- **暗色模式** — 跟随系统或手动切换，iOS 18 风格设计
- **系统托盘** — 桌面端关闭窗口后最小化到托盘，后台持续运行

## 安装

| 平台 | 方式 |
|------|------|
| macOS | 下载 `.dmg`，拖入 `/Applications` |
| Windows | 下载 `.msi`，双击安装 |
| Android | 下载 `.apk`，允许安装未知来源 |

→ [最新 Release](https://github.com/sun123hao/sharecopy/releases)

## 快速开始

1. 两台设备连接**同一 WiFi**
2. 分别打开 ShareCopy → 自动发现并显示在线设备（绿色圆点）
3. 在一台设备复制文字/图片 → 另一台设备自动收到
4. 点击设备卡片右侧 📤 或拖放文件到卡片 → 点对点传输

## 技术栈

| 层 | 技术 |
|---|------|
| 桌面框架 | Tauri 2.x |
| 前端 | React 19 + TypeScript + Tailwind CSS 4 |
| 后端 | Rust (Tokio) |
| 设备发现 | mDNS (Bonjour) |
| 通信协议 | 自定义二进制协议 (TCP:54322) |
| 文件校验 | SHA256, 256KB 分块 |

## 开发

```bash
# 安装依赖
npm install

# 启动开发模式
npm run dev

# 构建桌面端
npx tauri build

# 构建 Android
npm run android:build
```

## 项目结构

```
sharecopy/
├── src/                      # React 前端
│   ├── components/           # UI 组件
│   ├── hooks/                # Tauri 事件 & 命令封装
│   ├── store/                # Zustand 状态管理
│   ├── types/                # TypeScript 类型
│   └── utils/                # 工具函数
├── src-tauri/                # Rust 后端
│   ├── src/
│   │   ├── clipboard.rs      # 剪贴板监听与写入
│   │   ├── discovery.rs      # mDNS 设备发现
│   │   ├── network.rs        # TCP 网络管理
│   │   ├── sync.rs           # 同步引擎
│   │   ├── transfer.rs       # 文件传输
│   │   ├── protocol.rs       # 通信协议
│   │   └── config.rs         # 配置持久化
│   └── Cargo.toml
└── website/                  # 产品宣传站
```

## 隐私

- 所有数据仅在**局域网内传输**，不上传任何服务器
- 文件传输使用 SHA256 端到端校验
- 不收集用户数据，无需注册账号

## License

MIT © [sun123hao](https://github.com/sun123hao)
