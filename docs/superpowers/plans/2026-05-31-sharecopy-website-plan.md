# ShareCopy 官网实现计划

> **For agentic workers:** 使用 subagent-driven-development 或 executing-plans 来逐步实现此计划。步骤使用 checkbox (`- [ ]`) 语法跟踪进度。

**目标：** 构建一个 Apple 风格极简产品单页官网，纯静态 HTML/CSS/JS，零框架依赖。

**架构：** 单页 HTML，CSS 独立文件，少量原生 JS 处理滚动渐显和平滑导航。响应式三断点（桌面/平板/手机）。部署至 GitHub Pages。

**技术栈：** HTML5 + CSS3 + Vanilla JS，系统字体栈，无外部依赖。

---

## 文件结构

```
website/
├── index.html          # 单页 HTML 结构
├── style.css           # 全部样式（含响应式）
├── script.js           # 滚动渐显 + 平滑滚动
└── assets/
    └── hero.png        # Hero 区域产品图（从 src/assets/hero.png 复制）
```

---

### Task 1: 创建网站目录结构和 HTML 骨架

**文件：**
- 创建：`website/index.html`

- [ ] **Step 1: 创建目录结构**

```bash
mkdir -p website/assets
cp src/assets/hero.png website/assets/hero.png
```

- [ ] **Step 2: 编写 HTML 骨架（导航 + Hero）**

```html
<!DOCTYPE html>
<html lang="zh-CN">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ShareCopy — 跨设备局域网共享工具</title>
  <meta name="description" content="在同一局域网内，实时同步剪贴板、快速传输文件。无需联网、无需注册、无需服务器。">
  <link rel="icon" type="image/png" href="assets/hero.png">
  <link rel="stylesheet" href="style.css">
</head>
<body>

  <!-- 导航栏 -->
  <nav class="nav" id="nav">
    <div class="nav-inner">
      <a href="#" class="nav-logo">ShareCopy</a>
      <a href="https://github.com/sun123hao/sharecopy/releases" class="nav-download-btn">免费下载</a>
    </div>
  </nav>

  <!-- Hero -->
  <section class="hero">
    <div class="hero-content">
      <h1 class="hero-title">
        <span class="hero-title-line">跨设备</span>
        <span class="hero-title-line gradient-text">无缝共享</span>
      </h1>
      <p class="hero-subtitle">
        在同一局域网内，实时同步剪贴板、快速传输文件。<br>无需联网、无需注册、无需服务器。
      </p>
      <a href="https://github.com/sun123hao/sharecopy/releases" class="cta-btn">
        免费下载
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
      </a>
      <div class="hero-mockup">
        <img src="assets/hero.png" alt="ShareCopy 界面" class="hero-image">
      </div>
    </div>
  </section>

  <!-- 核心功能 -->
  <section class="features" id="features">
    <div class="section-header reveal">
      <h2 class="section-title">核心功能</h2>
    </div>
    <div class="features-grid">
      <div class="feature-card reveal">
        <div class="feature-icon">📋</div>
        <h3 class="feature-title">剪贴板实时同步</h3>
        <p class="feature-desc">文字、图片自动同步到局域网内所有设备，复制即共享，双向实时。</p>
      </div>
      <div class="feature-card reveal">
        <div class="feature-icon">📁</div>
        <h3 class="feature-title">点对点文件传输</h3>
        <p class="feature-desc">点击发送或拖放文件到设备卡片，SHA256 校验确保完整性，支持大文件。</p>
      </div>
      <div class="feature-card reveal">
        <div class="feature-icon">🔍</div>
        <h3 class="feature-title">自动设备发现</h3>
        <p class="feature-desc">基于 mDNS (Bonjour) 自动发现局域网内设备，无需手动配置 IP 地址。</p>
      </div>
    </div>
  </section>

  <!-- 支持平台 -->
  <section class="platforms" id="platforms">
    <div class="section-header reveal">
      <h2 class="section-title">支持平台</h2>
    </div>
    <div class="platforms-grid">
      <div class="platform-card reveal">
        <div class="platform-icon">🖥</div>
        <h3 class="platform-name">macOS</h3>
        <p class="platform-detail">Apple Silicon &amp; Intel</p>
        <a href="https://github.com/sun123hao/sharecopy/releases" class="platform-dl">下载 .dmg</a>
      </div>
      <div class="platform-card reveal">
        <div class="platform-icon">🪟</div>
        <h3 class="platform-name">Windows</h3>
        <p class="platform-detail">Windows 10 / 11</p>
        <a href="https://github.com/sun123hao/sharecopy/releases" class="platform-dl">下载 .msi</a>
      </div>
      <div class="platform-card reveal">
        <div class="platform-icon">📱</div>
        <h3 class="platform-name">Android</h3>
        <p class="platform-detail">Android 8.0+</p>
        <a href="https://github.com/sun123hao/sharecopy/releases" class="platform-dl">下载 .apk</a>
      </div>
    </div>
  </section>

  <!-- 三步开始 -->
  <section class="steps" id="steps">
    <div class="section-header reveal">
      <h2 class="section-title">三步开始</h2>
    </div>
    <div class="steps-list">
      <div class="step-item reveal">
        <div class="step-number">1</div>
        <h3 class="step-title">安装 ShareCopy</h3>
        <p class="step-desc">在你的 Mac、Windows 或 Android 设备上下载并安装。</p>
      </div>
      <div class="step-divider"></div>
      <div class="step-item reveal">
        <div class="step-number">2</div>
        <h3 class="step-title">同一网络自动发现</h3>
        <p class="step-desc">连接到同一个 WiFi，设备之间自动发现，无需手动配对。</p>
      </div>
      <div class="step-divider"></div>
      <div class="step-item reveal">
        <div class="step-number">3</div>
        <h3 class="step-title">开始共享</h3>
        <p class="step-desc">复制文字或拖放文件，即刻同步到其他设备，就这么简单。</p>
      </div>
    </div>
  </section>

  <!-- Footer -->
  <footer class="footer">
    <div class="footer-inner">
      <a href="https://github.com/sun123hao/sharecopy" class="footer-github" target="_blank" rel="noopener">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/></svg>
        GitHub
      </a>
      <p class="footer-copy">&copy; 2025 ShareCopy</p>
    </div>
  </footer>

  <script src="script.js"></script>
</body>
</html>
```

- [ ] **Step 3: 提交**

```bash
git add website/index.html website/assets/hero.png
git commit -m "feat: 创建官网 HTML 骨架和目录结构"
```

---

### Task 2: 编写样式 — 全局与导航

**文件：**
- 创建：`website/style.css`

- [ ] **Step 1: 编写 CSS 重置、全局变量和导航栏样式**

```css
/* ========================================
   CSS 自定义属性
   ======================================== */
:root {
  --color-bg: #ffffff;
  --color-text: #1d1d1f;
  --color-text-secondary: #86868b;
  --color-accent: #AF52DE;
  --color-accent-end: #5E5CE6;
  --color-card-bg: #f5f5f7;
  --color-border: rgba(0, 0, 0, 0.06);
  --font-stack: -apple-system, BlinkMacSystemFont, "SF Pro Display", "SF Pro Text",
                "Helvetica Neue", Helvetica, Arial, "PingFang SC", "Hiragino Sans GB",
                "Microsoft YaHei", sans-serif;
  --max-width: 1024px;
  --nav-height: 52px;
}

/* ========================================
   CSS 重置
   ======================================== */
*,
*::before,
*::after {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html {
  scroll-behavior: smooth;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  font-family: var(--font-stack);
  color: var(--color-text);
  background: var(--color-bg);
  line-height: 1.6;
  overflow-x: hidden;
}

a {
  text-decoration: none;
  color: inherit;
}

img {
  max-width: 100%;
  display: block;
}

/* ========================================
   导航栏
   ======================================== */
.nav {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  z-index: 100;
  height: var(--nav-height);
  background: rgba(255, 255, 255, 0.8);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border-bottom: 1px solid var(--color-border);
}

.nav-inner {
  max-width: var(--max-width);
  margin: 0 auto;
  padding: 0 24px;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.nav-logo {
  font-size: 20px;
  font-weight: 700;
  letter-spacing: -0.3px;
  color: var(--color-text);
}

.nav-download-btn {
  font-size: 14px;
  font-weight: 600;
  color: #fff;
  background: linear-gradient(135deg, var(--color-accent), var(--color-accent-end));
  padding: 8px 20px;
  border-radius: 20px;
  transition: opacity 0.2s, transform 0.2s;
}

.nav-download-btn:hover {
  opacity: 0.9;
  transform: scale(1.03);
}
```

- [ ] **Step 2: 提交**

```bash
git add website/style.css
git commit -m "feat: 添加官网全局样式和导航栏"
```

---

### Task 3: Hero 区域样式

**文件：**
- 修改：`website/style.css`（追加 Hero 样式）

- [ ] **Step 1: 追加 Hero 区域 CSS**

在 `style.css` 末尾追加：

```css
/* ========================================
   Hero
   ======================================== */
.hero {
  padding: 120px 24px 80px;
  text-align: center;
}

.hero-content {
  max-width: 680px;
  margin: 0 auto;
}

.hero-title {
  font-size: clamp(40px, 8vw, 64px);
  font-weight: 800;
  letter-spacing: -1.5px;
  line-height: 1.1;
  margin-bottom: 20px;
}

.hero-title-line {
  display: block;
}

.gradient-text {
  background: linear-gradient(135deg, var(--color-accent), var(--color-accent-end));
  -webkit-background-clip: text;
  -webkit-text-fill-color: transparent;
  background-clip: text;
}

.hero-subtitle {
  font-size: clamp(17px, 2.5vw, 21px);
  color: var(--color-text-secondary);
  line-height: 1.6;
  margin-bottom: 36px;
}

/* CTA 按钮 */
.cta-btn {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 18px;
  font-weight: 600;
  color: #fff;
  background: linear-gradient(135deg, var(--color-accent), var(--color-accent-end));
  padding: 14px 36px;
  border-radius: 28px;
  transition: opacity 0.2s, transform 0.2s, box-shadow 0.2s;
  box-shadow: 0 4px 20px rgba(175, 82, 222, 0.3);
}

.cta-btn:hover {
  opacity: 0.92;
  transform: scale(1.03);
  box-shadow: 0 6px 28px rgba(175, 82, 222, 0.4);
}

/* Hero 产品图 */
.hero-mockup {
  margin-top: 64px;
}

.hero-image {
  width: 100%;
  max-width: 720px;
  margin: 0 auto;
  border-radius: 16px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.08);
}
```

- [ ] **Step 2: 提交**

```bash
git add website/style.css
git commit -m "feat: 添加 Hero 区域样式"
```

---

### Task 4: 功能卡片区域样式

**文件：**
- 修改：`website/style.css`（追加 Features 样式）

- [ ] **Step 1: 追加功能卡片 CSS**

在 `style.css` 末尾追加：

```css
/* ========================================
   通用 Section 标题
   ======================================== */
.section-header {
  text-align: center;
  margin-bottom: 48px;
}

.section-title {
  font-size: clamp(28px, 5vw, 40px);
  font-weight: 700;
  letter-spacing: -0.5px;
}

/* ========================================
   核心功能
   ======================================== */
.features {
  padding: 80px 24px;
  max-width: var(--max-width);
  margin: 0 auto;
}

.features-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 24px;
}

.feature-card {
  background: var(--color-card-bg);
  border-radius: 20px;
  padding: 36px 28px;
  text-align: center;
  transition: transform 0.3s ease, box-shadow 0.3s ease;
}

.feature-card:hover {
  transform: translateY(-6px);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.06);
}

.feature-icon {
  font-size: 40px;
  margin-bottom: 16px;
}

.feature-title {
  font-size: 20px;
  font-weight: 700;
  margin-bottom: 8px;
  letter-spacing: -0.3px;
}

.feature-desc {
  font-size: 15px;
  color: var(--color-text-secondary);
  line-height: 1.6;
}

/* 响应式：平板及以下改为两列 */
@media (max-width: 768px) {
  .features-grid {
    grid-template-columns: repeat(2, 1fr);
    gap: 16px;
  }
}

/* 响应式：手机改为单列 */
@media (max-width: 480px) {
  .features-grid {
    grid-template-columns: 1fr;
  }
}
```

- [ ] **Step 2: 提交**

```bash
git add website/style.css
git commit -m "feat: 添加功能卡片区域样式"
```

---

### Task 5: 平台、步骤和 Footer 样式

**文件：**
- 修改：`website/style.css`（追加 Platform、Steps、Footer 样式）

- [ ] **Step 1: 追加平台、步骤、Footer 的 CSS**

在 `style.css` 末尾追加：

```css
/* ========================================
   支持平台
   ======================================== */
.platforms {
  padding: 80px 24px;
  background: var(--color-card-bg);
}

.platforms-grid {
  max-width: var(--max-width);
  margin: 0 auto;
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 24px;
}

.platform-card {
  text-align: center;
  padding: 32px 24px;
  background: #fff;
  border-radius: 20px;
  transition: transform 0.3s ease, box-shadow 0.3s ease;
}

.platform-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 8px 30px rgba(0, 0, 0, 0.04);
}

.platform-icon {
  font-size: 44px;
  margin-bottom: 12px;
}

.platform-name {
  font-size: 22px;
  font-weight: 700;
  margin-bottom: 4px;
}

.platform-detail {
  font-size: 14px;
  color: var(--color-text-secondary);
  margin-bottom: 20px;
}

.platform-dl {
  display: inline-block;
  font-size: 14px;
  font-weight: 600;
  color: var(--color-accent);
  padding: 8px 24px;
  border: 1.5px solid var(--color-accent);
  border-radius: 20px;
  transition: background 0.2s, color 0.2s;
}

.platform-dl:hover {
  background: var(--color-accent);
  color: #fff;
}

/* ========================================
   三步开始
   ======================================== */
.steps {
  padding: 80px 24px;
  max-width: 800px;
  margin: 0 auto;
}

.steps-list {
  display: flex;
  align-items: flex-start;
  justify-content: center;
  gap: 0;
}

.step-item {
  text-align: center;
  flex: 1;
}

.step-number {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: linear-gradient(135deg, var(--color-accent), var(--color-accent-end));
  color: #fff;
  font-size: 22px;
  font-weight: 700;
  margin-bottom: 16px;
}

.step-title {
  font-size: 18px;
  font-weight: 700;
  margin-bottom: 6px;
}

.step-desc {
  font-size: 14px;
  color: var(--color-text-secondary);
  line-height: 1.5;
}

.step-divider {
  width: 40px;
  height: 2px;
  background: var(--color-border);
  margin-top: 24px;
  flex-shrink: 0;
}

/* ========================================
   Footer
   ======================================== */
.footer {
  padding: 32px 24px;
  border-top: 1px solid var(--color-border);
}

.footer-inner {
  max-width: var(--max-width);
  margin: 0 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 24px;
}

.footer-github {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 14px;
  color: var(--color-text-secondary);
  transition: color 0.2s;
}

.footer-github:hover {
  color: var(--color-text);
}

.footer-copy {
  font-size: 14px;
  color: var(--color-text-secondary);
}

/* ========================================
   响应式：Steps
   ======================================== */
@media (max-width: 640px) {
  .steps-list {
    flex-direction: column;
    align-items: center;
    gap: 32px;
  }

  .step-divider {
    width: 2px;
    height: 24px;
  }

  .platforms-grid {
    grid-template-columns: 1fr;
    max-width: 360px;
    margin: 0 auto;
  }
}
```

- [ ] **Step 2: 提交**

```bash
git add website/style.css
git commit -m "feat: 添加平台、步骤和 Footer 样式"
```

---

### Task 6: 滚动渐显动画脚本

**文件：**
- 创建：`website/script.js`

- [ ] **Step 1: 编写 Intersection Observer 滚动渐显脚本**

```javascript
// 滚动渐显动画 — 使用 Intersection Observer
(function () {
  const observerOptions = {
    root: null,
    rootMargin: '0px 0px -60px 0px',
    threshold: 0.1,
  };

  const observer = new IntersectionObserver((entries) => {
    entries.forEach((entry) => {
      if (entry.isIntersecting) {
        entry.target.style.opacity = '1';
        entry.target.style.transform = 'translateY(0)';
        observer.unobserve(entry.target);
      }
    });
  }, observerOptions);

  // 为所有 .reveal 元素添加初始隐藏样式并注册观察
  document.querySelectorAll('.reveal').forEach((el) => {
    el.style.opacity = '0';
    el.style.transform = 'translateY(24px)';
    el.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
    observer.observe(el);
  });
})();

// 导航栏滚动阴影
(function () {
  const nav = document.getElementById('nav');
  if (!nav) return;

  window.addEventListener('scroll', () => {
    if (window.scrollY > 10) {
      nav.style.boxShadow = '0 1px 8px rgba(0,0,0,0.04)';
    } else {
      nav.style.boxShadow = 'none';
    }
  });
})();
```

- [ ] **Step 2: 提交**

```bash
git add website/script.js
git commit -m "feat: 添加滚动渐显动画和导航栏阴影脚本"
```

---

### Task 7: 响应式微调与整体验证

**文件：**
- 修改：`website/style.css`（追加响应式细节）

- [ ] **Step 1: 追加平板和手机的额外响应式样式**

在 `style.css` 末尾追加：

```css
/* ========================================
   响应式：平板
   ======================================== */
@media (max-width: 768px) {
  .hero {
    padding: 100px 20px 60px;
  }

  .hero-mockup {
    margin-top: 48px;
  }

  .features {
    padding: 60px 20px;
  }

  .platforms {
    padding: 60px 20px;
  }

  .steps {
    padding: 60px 20px;
  }

  .feature-card {
    padding: 28px 20px;
  }
}
```

- [ ] **Step 2: 用 Live Server 或直接打开浏览器验证**

```bash
# macOS 直接打开预览
open website/index.html
```

目视检查：
- 桌面端（>1024px）：三列卡片、横向步骤
- 平板（768-1023px）：功能两列、平台三列
- 手机（<768px）：功能单列、步骤纵向、平台单列
- 导航栏毛玻璃效果、hover 过渡
- 滚动时卡片逐个渐显

- [ ] **Step 3: 提交**

```bash
git add website/style.css
git commit -m "feat: 添加响应式微调，完成官网样式"
```

---

### Task 8: GitHub Pages 部署配置

**文件：**
- 创建：`.github/workflows/deploy-website.yml`

- [ ] **Step 1: 创建 GitHub Actions 部署工作流**

```yaml
name: Deploy Website to GitHub Pages

on:
  push:
    branches: [main]
    paths:
      - 'website/**'
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: "pages"
  cancel-in-progress: false

jobs:
  deploy:
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    runs-on: ubuntu-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Setup Pages
        uses: actions/configure-pages@v4

      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: 'website'

      - name: Deploy to GitHub Pages
        id: deployment
        uses: actions/deploy-pages@v4
```

- [ ] **Step 2: 提交**

```bash
git add .github/workflows/deploy-website.yml
git commit -m "ci: 添加 GitHub Pages 自动部署工作流"
```
