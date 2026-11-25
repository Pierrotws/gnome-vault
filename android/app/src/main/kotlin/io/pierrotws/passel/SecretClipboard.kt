package io.pierrotws.passel

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Copies secrets like `pass -c`: marked sensitive (keyboards and the
 * Android 13+ preview hide it) and cleared after [CLEAR_AFTER_SECONDS],
 * unless something else was copied meanwhile.
 */
class SecretClipboard(context: Context, private val scope: CoroutineScope) {
    private val clipboard = context.getSystemService(ClipboardManager::class.java)
    private var clearing: Job? = null

    /** Returns false on Android 13+, which shows its own confirmation. */
    fun copy(text: String): Boolean {
        val clip = ClipData.newPlainText(LABEL, text)
        clip.description.extras = PersistableBundle().apply {
            putBoolean(
                if (Build.VERSION.SDK_INT >= 33) ClipDescription.EXTRA_IS_SENSITIVE else "android.content.extra.IS_SENSITIVE",
                true,
            )
        }
        clipboard.setPrimaryClip(clip)
        clearing?.cancel()
        clearing = scope.launch {
            delay(CLEAR_AFTER_SECONDS * 1000L)
            // Our label still on top: nothing else was copied since.
            if (clipboard.primaryClipDescription?.label == LABEL) {
                if (Build.VERSION.SDK_INT >= 28) {
                    clipboard.clearPrimaryClip()
                } else {
                    clipboard.setPrimaryClip(ClipData.newPlainText("", ""))
                }
            }
        }
        return Build.VERSION.SDK_INT < 33
    }

    companion object {
        const val CLEAR_AFTER_SECONDS = 45
        private const val LABEL = "Passel"
    }
}
