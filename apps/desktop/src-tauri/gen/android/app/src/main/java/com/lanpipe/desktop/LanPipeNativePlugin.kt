package com.lanpipe.desktop

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.util.Log
import android.webkit.MimeTypeMap
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.io.FileOutputStream
import java.io.IOException

/**
 * LanPipe Android 原生桥接插件。
 *
 * 与 Rust 侧 `src/mobile.rs` 中的 `NativeBridge` 一一对应，命令名必须保持一致：
 * - `getInsets`      -> WindowInsets（dp），Rust 侧轮询后 emit `android:insets`
 * - `pickFiles`      -> SAF ACTION_OPEN_DOCUMENT 多选，拷贝到 cacheDir/imports/<ts>/
 * - `pickFolder`     -> SAF ACTION_OPEN_DOCUMENT_TREE，整棵目录树拷贝到缓存后返回本地路径
 * - `openPath`       -> 用系统应用打开本地文件（FileProvider）
 * - `startForeground`/`stopForeground` -> dataSync 前台服务
 *
 * 说明：SAF 返回的是 `content://` URI，WebView/网络层无法直接读取，因此必须先拷贝到应用私有缓存，
 * 再把绝对路径交给 Rust/前端；拷贝是同步的，在 Activity 回调线程上执行。
 */
@TauriPlugin
class LanPipeNativePlugin(private val appActivity: Activity) : Plugin(appActivity) {

    companion object {
        private const val TAG = "lanpipe-native"
        private const val MAX_TREE_DEPTH = 24
    }

    // ------------------------------------------------------------------
    // 日志：Rust 侧 `native_log` 转发到 logcat（tag lanpipe-native）
    // ------------------------------------------------------------------

    @Command
    fun log(invoke: Invoke) {
        val msg: String = invoke.getArgs().getString("msg") ?: ""
        if (msg.isNotEmpty()) {
            Log.i(TAG, msg)
        }
        invoke.resolve()
    }

    // ------------------------------------------------------------------
    // WindowInsets
    // ------------------------------------------------------------------

    @Command
    fun getInsets(invoke: Invoke) {
        invoke.resolve(
            JSObject().apply {
                put("top", MainActivity.insetsTopDp)
                put("bottom", MainActivity.insetsBottomDp)
                put("ime", MainActivity.insetsImeDp)
            }
        )
    }

    // ------------------------------------------------------------------
    // SAF：选择文件
    // ------------------------------------------------------------------

    @Command
    fun pickFiles(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
        }
        Log.i(TAG, "pickFiles: launching ACTION_OPEN_DOCUMENT")
        startActivityForResult(invoke, intent, "pickFilesResult")
    }

    @ActivityCallback
    fun pickFilesResult(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            Log.i(TAG, "pickFiles: cancelled")
            invoke.resolve(pathsPayload(emptyList()))
            return
        }
        val data = result.data
        val uris = ArrayList<Uri>()
        val clip = data?.clipData
        if (clip != null) {
            for (i in 0 until clip.itemCount) uris.add(clip.getItemAt(i).uri)
        } else {
            data?.data?.let { uris.add(it) }
        }
        val destDir = newImportDir()
        val out = ArrayList<String>()
        for (uri in uris) {
            try {
                out.add(copyUriToFile(uri, destDir, null).absolutePath)
            } catch (e: Exception) {
                Log.w(TAG, "pickFiles: copy failed for $uri", e)
            }
        }
        Log.i(TAG, "pickFiles: copied ${out.size}/${uris.size} file(s) into ${destDir.absolutePath}")
        invoke.resolve(pathsPayload(out))
    }

    // ------------------------------------------------------------------
    // SAF：选择目录（递归拷贝）
    // ------------------------------------------------------------------

    @Command
    fun pickFolder(invoke: Invoke) {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
            )
        }
        Log.i(TAG, "pickFolder: launching ACTION_OPEN_DOCUMENT_TREE")
        startActivityForResult(invoke, intent, "pickFolderResult")
    }

    @ActivityCallback
    fun pickFolderResult(invoke: Invoke, result: ActivityResult) {
        val treeUri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || treeUri == null) {
            Log.i(TAG, "pickFolder: cancelled")
            invoke.resolve(pathsPayload(emptyList()))
            return
        }
        try {
            appActivity.contentResolver.takePersistableUriPermission(
                treeUri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION
            )
        } catch (e: Exception) {
            Log.w(TAG, "pickFolder: takePersistableUriPermission failed", e)
        }
        return try {
            val rootDocId = DocumentsContract.getTreeDocumentId(treeUri)
            val rootName = queryDisplayName(DocumentsContract.buildDocumentUriUsingTree(treeUri, rootDocId))
                ?: "folder_${System.currentTimeMillis()}"
            val destDir = File(newImportDir(), sanitizeName(rootName)).apply { mkdirs() }
            val copied = walkTree(treeUri, rootDocId, destDir, 0)
            Log.i(TAG, "pickFolder: copied $copied file(s) into ${destDir.absolutePath}")
            invoke.resolve(pathsPayload(listOf(destDir.absolutePath)))
        } catch (e: Exception) {
            Log.w(TAG, "pickFolder: failed", e)
            invoke.reject(e.message ?: "无法读取所选目录")
        }
    }

    private fun walkTree(treeUri: Uri, parentDocId: String, destDir: File, depth: Int): Int {
        if (depth > MAX_TREE_DEPTH) {
            Log.w(TAG, "walkTree: max depth reached at $parentDocId")
            return 0
        }
        var copied = 0
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        appActivity.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            while (cursor.moveToNext()) {
                val childId = cursor.getString(0) ?: continue
                val name = cursor.getString(1) ?: "unnamed"
                val mime = cursor.getString(2)
                val docUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, childId)
                if (mime == DocumentsContract.Document.MIME_TYPE_DIR) {
                    val subDir = File(destDir, sanitizeName(name)).apply { mkdirs() }
                    copied += walkTree(treeUri, childId, subDir, depth + 1)
                } else {
                    try {
                        copyUriToFile(docUri, destDir, name)
                        copied++
                    } catch (e: Exception) {
                        Log.w(TAG, "walkTree: copy failed for $docUri", e)
                    }
                }
            }
        }
        return copied
    }

    // ------------------------------------------------------------------
    // 用系统应用打开本地文件
    // ------------------------------------------------------------------

    @Command
    fun openPath(invoke: Invoke) {
        val args = invoke.getArgs()
        val path = args.getString("path", null)
        val mode = args.getString("mode", "file") ?: "file"
        if (path.isNullOrBlank()) {
            invoke.reject("openPath: path is required")
            return
        }
        return try {
            appActivity.startActivity(viewIntentFor(path, mode == "folder"))
            Log.i(TAG, "openPath: $path ($mode)")
            invoke.resolve()
        } catch (e: Exception) {
            Log.w(TAG, "openPath failed: $path", e)
            invoke.reject(e.message ?: "无法打开该路径")
        }
    }

    private fun viewIntentFor(rawPath: String, folderMode: Boolean): Intent {
        val isContentUri = rawPath.startsWith("content://")
        val file = if (isContentUri) null else File(rawPath.removePrefix("file://"))
        val uri: Uri = if (isContentUri) {
            Uri.parse(rawPath)
        } else {
            FileProvider.getUriForFile(
                appActivity,
                "${appActivity.packageName}.fileprovider",
                file!!
            )
        }
        val mime = when {
            folderMode || (file != null && file.isDirectory) -> "resource/folder"
            isContentUri -> appActivity.contentResolver.getType(uri) ?: "*/*"
            else -> mimeOf(file!!)
        }
        return Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, mime)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
    }

    private fun mimeOf(file: File): String {
        val ext = file.extension.lowercase()
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
    }

    // ------------------------------------------------------------------
    // dataSync 前台服务
    // ------------------------------------------------------------------

    @Command
    fun startForeground(invoke: Invoke) {
        return try {
            LanPipeSyncService.start(appActivity.applicationContext)
            Log.i(TAG, "startForeground: dataSync service requested")
            invoke.resolve()
        } catch (e: Exception) {
            Log.w(TAG, "startForeground failed", e)
            invoke.reject(e.message ?: "无法启动前台服务")
        }
    }

    @Command
    fun stopForeground(invoke: Invoke) {
        return try {
            LanPipeSyncService.stop(appActivity.applicationContext)
            Log.i(TAG, "stopForeground: dataSync service stopped")
            invoke.resolve()
        } catch (e: Exception) {
            Log.w(TAG, "stopForeground failed", e)
            invoke.reject(e.message ?: "无法停止前台服务")
        }
    }

    // ------------------------------------------------------------------
    // 工具方法
    // ------------------------------------------------------------------

    private fun pathsPayload(paths: List<String>): JSObject {
        val array = JSArray()
        for (path in paths) array.put(path)
        return JSObject().put("paths", array)
    }

    /** 每次选取生成独立的导入目录：cacheDir/imports/<时间戳>/。 */
    private fun newImportDir(): File {
        val dir = File(File(appActivity.cacheDir, "imports"), System.currentTimeMillis().toString())
        if (!dir.exists() && !dir.mkdirs()) {
            throw IOException("无法创建导入目录 ${dir.absolutePath}")
        }
        return dir
    }

    private fun copyUriToFile(uri: Uri, destDir: File, preferredName: String?): File {
        if (!destDir.exists()) destDir.mkdirs()
        val displayName = preferredName ?: queryDisplayName(uri) ?: "imported_${System.currentTimeMillis()}"
        val safeName = sanitizeName(displayName)
        var target = File(destDir, safeName)
        var index = 1
        while (target.exists()) {
            target = File(destDir, "${index}_$safeName")
            index++
        }
        val input = appActivity.contentResolver.openInputStream(uri)
            ?: throw IOException("无法读取 $uri")
        input.use { source ->
            FileOutputStream(target).use { output -> source.copyTo(output) }
        }
        return target
    }

    private fun queryDisplayName(uri: Uri): String? {
        return try {
            appActivity.contentResolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (index >= 0) cursor.getString(index) else null
                } else {
                    null
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "queryDisplayName failed for $uri", e)
            null
        }
    }

    private fun sanitizeName(name: String): String {
        val cleaned = name.replace(Regex("[\\\\/:*?\"<>|\\u0000-\\u001F]"), "_").trim()
        return if (cleaned.isEmpty() || cleaned == "." || cleaned == "..") "unnamed" else cleaned.take(120)
    }
}
