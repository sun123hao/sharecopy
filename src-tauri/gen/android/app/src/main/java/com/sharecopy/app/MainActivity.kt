package com.sharecopy.app

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // 启动前台服务，保持 TCP 服务器和剪贴板同步在后台运行
    startSyncService()

    // 请求电池优化豁免（防止息屏被杀）
    requestBatteryExemption()
  }

  private fun startSyncService() {
    val intent = Intent(this, ShareCopyService::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      startForegroundService(intent)
    } else {
      startService(intent)
    }
  }

  private fun requestBatteryExemption() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
      val pm = getSystemService(POWER_SERVICE) as PowerManager
      if (!pm.isIgnoringBatteryOptimizations(packageName)) {
        try {
          val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS).apply {
            data = Uri.parse("package:$packageName")
          }
          startActivity(intent)
        } catch (e: Exception) {
          android.util.Log.w("ShareCopy", "电池优化豁免请求失败", e)
        }
      }
    }
  }
}
