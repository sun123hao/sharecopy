package com.sharecopy.app

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat

/**
 * ShareCopy 前台服务
 *
 * 保持应用在后台持续运行，确保 TCP 服务器和剪贴板同步不中断。
 * Android 8+ 要求显示常驻通知，告知用户服务正在运行。
 */
class ShareCopyService : Service() {

    companion object {
        const val CHANNEL_ID = "sharecopy_sync"
        const val NOTIFICATION_ID = 1001
        const val ACTION_STOP = "com.sharecopy.app.STOP_SERVICE"

        /** 启动前台服务，统一处理版本兼容 */
        fun start(context: Context) {
            val intent = Intent(context, ShareCopyService::class.java)
            try {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    context.startForegroundService(intent)
                } else {
                    context.startService(intent)
                }
            } catch (e: Exception) {
                android.util.Log.w("ShareCopy", "启动服务失败", e)
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }

        val notification = buildNotification()
        startForeground(NOTIFICATION_ID, notification)

        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "剪贴板同步",
                NotificationManager.IMPORTANCE_LOW // LOW = 不发出声音，仅显示在通知栏
            ).apply {
                description = "ShareCopy 剪贴板同步服务运行中"
                setShowBadge(false)
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        // 点击通知打开主界面
        val openIntent = Intent(this, MainActivity::class.java).let {
            PendingIntent.getActivity(
                this,
                0,
                it,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )
        }

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("ShareCopy 同步中")
            .setContentText("正在监听剪贴板变化...")
            .setSmallIcon(android.R.drawable.ic_menu_share)
            .setOngoing(true) // 用户无法滑动删除
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setContentIntent(openIntent)
            .build()
    }
}
