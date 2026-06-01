package com.sharecopy.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * 系统事件接收器：开机、亮屏时自动启动前台服务
 */
class ShareCopyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_SCREEN_ON,
            Intent.ACTION_USER_PRESENT -> {
                ShareCopyService.start(context)
            }
        }
    }
}
