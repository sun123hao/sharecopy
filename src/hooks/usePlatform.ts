import { useState } from 'react';

export type Platform = 'desktop' | 'android' | 'ios' | 'unknown';

let cached: Platform | null = null;

/** 检测当前运行平台 */
export function detectPlatform(): Platform {
  if (cached) return cached;

  // Tauri 注入的 window.__TAURI_INTERNALS__ 可用于判断是否在 Tauri 环境
  const hasTauri = '__TAURI_INTERNALS__' in window;

  if (!hasTauri) {
    // 浏览器开发模式：根据 userAgent 推测
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('android')) {
      cached = 'android';
    } else if (ua.includes('iphone') || ua.includes('ipad')) {
      cached = 'ios';
    } else {
      cached = 'desktop';
    }
    return cached;
  }

  // Tauri 环境：根据操作系统判断
  // Tauri 在移动端会设置相应的 OS 标识
  if (navigator.userAgent.includes('Android')) {
    cached = 'android';
  } else if (navigator.userAgent.includes('iPhone') || navigator.userAgent.includes('iPad')) {
    cached = 'ios';
  } else {
    cached = 'desktop';
  }

  return cached;
}

/** React hook：获取当前平台 */
export function usePlatform(): Platform {
  const [platform] = useState<Platform>(() => detectPlatform());
  return platform;
}

/** 是否为移动端 */
export function isMobile(): boolean {
  const p = detectPlatform();
  return p === 'android' || p === 'ios';
}

/** 是否为桌面端 */
export function isDesktop(): boolean {
  return detectPlatform() === 'desktop';
}
