package com.sharecopy.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build

/**
 * 系统事件接收器：开机、亮屏时自动启动前台服务
 */
class ShareCopyReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED,
            Intent.ACTION_SCREEN_ON,
            Intent.ACTION_USER_PRESENT -> {
                startSyncService(context)
            }
        }
    }

    private fun startSyncService(context: Context) {
        val serviceIntent = Intent(context, ShareCopyService::class.java)
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(serviceIntent)
            } else {
                context.startService(serviceIntent)
            }
        } catch (e: Exception) {
            android.util.Log.w("ShareCopy", "自动启动服务失败", e)
        }
    }
}
