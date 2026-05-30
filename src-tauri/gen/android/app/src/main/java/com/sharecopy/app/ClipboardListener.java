package com.sharecopy.app;

import android.content.ClipboardManager;

/**
 * 剪贴板变更监听器 — 后台也能收到通知
 * onPrimaryClipChanged() 调用 Rust 端的 native 方法递增计数器
 */
public class ClipboardListener implements ClipboardManager.OnPrimaryClipChangedListener {

    /**
     * JNI native 方法 — 实现在 Rust clipboard_android.rs
     * 每次剪贴板变更时递增 CLIP_CHANGE_COUNT
     */
    public static native void onClipboardChanged();

    @Override
    public void onPrimaryClipChanged() {
        onClipboardChanged();
    }
}
