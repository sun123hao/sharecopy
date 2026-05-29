package com.sharecopy.app

import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // 启动前台服务，保持 TCP 服务器和剪贴板同步在后台运行
    startSyncService()
  }

  private fun startSyncService() {
    val intent = Intent(this, ShareCopyService::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      startForegroundService(intent)
    } else {
      startService(intent)
    }
  }
}
