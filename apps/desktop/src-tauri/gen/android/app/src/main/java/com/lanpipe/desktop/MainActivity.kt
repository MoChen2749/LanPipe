package com.lanpipe.desktop

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.roundToInt

/**
 * LanPipe Android 主 Activity。
 *
 * 在生成的 `TauriActivity` 基础上补齐 Android 平台能力：
 * - edge-to-edge + WindowInsets 采集（Rust 侧轮询后 emit `android:insets`）；
 * - 返回键：优先派发前端监听的 `android:back` DOM 事件（Android 13+ 走 OnBackPressedDispatcher，
 *   低版本同样由 dispatcher 兜底，无需区分实现）；
 * - 前台 WifiManager.MulticastLock：onResume 获取 / onPause 释放，用于局域网 UDP 多播发现；
 * - Android 13+ 动态申请 POST_NOTIFICATIONS（前台服务通知需要）。
 *
 * 其余能力（SAF、前台服务、insets 读取）在 [LanPipeNativePlugin] 中实现。
 */
class MainActivity : TauriActivity() {

    companion object {
        private const val TAG = "lanpipe-native"
        private const val MULTICAST_LOCK_TAG = "lanpipe-multicast"

        /** 供 LanPipeNativePlugin#getInsets 读取，单位 dp。 */
        @Volatile
        var insetsTopDp: Int = 0
            private set
        @Volatile
        var insetsBottomDp: Int = 0
            private set
        @Volatile
        var insetsImeDp: Int = 0
            private set
    }

    private var webView: WebView? = null
    private var multicastLock: WifiManager.MulticastLock? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    val view = webView
                    if (view == null) {
                        // WebView 尚未就绪：退到后台，避免返回键完全失效。
                        Log.w(TAG, "back pressed before webview ready; moving task to back")
                        moveTaskToBack(true)
                        return
                    }
                    // 前端 src/main.ts 监听 window 上的 "android:back" 事件并执行 history.back()。
                    view.evaluateJavascript(
                        "window.dispatchEvent(new CustomEvent('android:back'))",
                        null
                    )
                }
            }
        )

        requestNotificationPermissionIfNeeded()
    }

    override fun onWebViewCreate(webView: WebView) {
        super.onWebViewCreate(webView)
        this.webView = webView
        val root = findViewById<View>(android.R.id.content) ?: webView
        ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
            val density = resources.displayMetrics.density.coerceAtLeast(0.1f)
            insetsTopDp = (bars.top / density).roundToInt()
            insetsBottomDp = (bars.bottom / density).roundToInt()
            insetsImeDp = (ime.bottom / density).roundToInt()
            insets
        }
        ViewCompat.requestApplyInsets(root)
    }

    override fun onResume() {
        super.onResume()
        acquireMulticastLock()
    }

    override fun onPause() {
        releaseMulticastLock()
        super.onPause()
    }

    override fun onDestroy() {
        releaseMulticastLock()
        webView = null
        super.onDestroy()
    }

    private fun acquireMulticastLock() {
        if (multicastLock?.isHeld == true) return
        try {
            val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            if (wifi == null) {
                Log.w(TAG, "multicast lock: WifiManager unavailable")
                return
            }
            val lock = multicastLock ?: wifi.createMulticastLock(MULTICAST_LOCK_TAG).also {
                it.setReferenceCounted(false)
            }
            lock.acquire()
            multicastLock = lock
            Log.i(TAG, "multicast lock acquired")
        } catch (e: Exception) {
            Log.w(TAG, "multicast lock acquire failed", e)
        }
    }

    private fun releaseMulticastLock() {
        try {
            val lock = multicastLock
            if (lock != null && lock.isHeld) {
                lock.release()
                Log.i(TAG, "multicast lock released")
            }
        } catch (e: Exception) {
            Log.w(TAG, "multicast lock release failed", e)
        }
    }

    private fun requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val granted = ContextCompat.checkSelfPermission(
            this,
            Manifest.permission.POST_NOTIFICATIONS
        ) == PackageManager.PERMISSION_GRANTED
        if (granted) return
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS)) { result ->
            Log.i(TAG, "POST_NOTIFICATIONS granted=$result")
        }
    }
}
