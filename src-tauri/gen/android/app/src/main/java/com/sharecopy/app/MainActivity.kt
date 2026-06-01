package com.sharecopy.app

import android.app.Dialog
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.View
import android.view.Window
import android.widget.Toast
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  companion object {
    private const val PREFS_NAME = "sharecopy_prefs"
    private const val KEY_FIRST_LAUNCH = "first_launch_done"
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // 启动前台服务，保持 TCP 服务器和剪贴板同步在后台运行
    ShareCopyService.start(this)

    // 首次启动：引导用户开启必要权限（剪贴板、电池优化）
    val prefs = getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
    if (!prefs.getBoolean(KEY_FIRST_LAUNCH, false)) {
      showPermissionGuide(prefs)
    }
  }

  /** 首次启动时弹出悬浮圆角对话框，引导用户进入 App 权限设置页 */
  private fun showPermissionGuide(prefs: SharedPreferences) {
    val dialog = Dialog(this)
    dialog.requestWindowFeature(Window.FEATURE_NO_TITLE)
    dialog.setContentView(R.layout.bottom_sheet_permission)

    dialog.window?.apply {
      setBackgroundDrawable(ColorDrawable(Color.TRANSPARENT))
      setGravity(Gravity.BOTTOM)
      attributes = attributes.apply {
        width = (resources.displayMetrics.widthPixels * 88 / 100)
        y = dpToPx(16)
      }
      setDimAmount(0.45f)
    }

    // 取消按钮 — 不标记完成，下次启动仍会弹出引导
    dialog.findViewById<View>(R.id.btn_cancel)?.setOnClickListener {
      dialog.dismiss()
    }

    // 去设置按钮 — 仅在用户真正前往设置后才标记已完成
    dialog.findViewById<View>(R.id.btn_go_settings)?.setOnClickListener {
      dialog.dismiss()
      prefs.edit().putBoolean(KEY_FIRST_LAUNCH, true).apply()
      openAppPermissionSettings()
    }

    dialog.show()
  }

  /** dp 转 px */
  private fun dpToPx(dp: Int): Int = (dp * resources.displayMetrics.density).toInt()

  /** 打开 App 系统详情页，用户可在此页面开启剪贴板、电池优化等所有权限 */
  private fun openAppPermissionSettings() {
    try {
      val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
        data = Uri.parse("package:$packageName")
      }
      startActivity(intent)
    } catch (e: Exception) {
      android.util.Log.w("ShareCopy", "打开权限设置失败", e)
      Toast.makeText(this, getString(R.string.toast_permission_failed), Toast.LENGTH_LONG).show()
    }
  }
}
