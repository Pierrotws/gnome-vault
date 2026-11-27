package io.pierrotws.passel

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import io.pierrotws.passel.core.Passel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.launch

/**
 * Process-wide objects. The Rust side keeps one keyring and one set of
 * credentials per process, so [passel] lives here rather than in an
 * activity.
 */
class PasselApp : Application() {
    /** Outlives activities: clipboard clearing, background syncs. */
    val scope = MainScope()

    val biometrics by lazy { BiometricUnlock(this) }

    val prompts by lazy { PromptBroker(biometrics) }

    @Volatile private var opened = false

    /** Opens the keyring and vault under `files/passel`. Blocks: first use off the main thread. */
    val passel: Passel by lazy { Passel(filesDir.resolve("passel").path, prompts).also { opened = true } }

    val remoteSecrets by lazy { RemoteSecrets(this) }

    val driveAuth by lazy { DriveAuth(this) }

    val clipboard by lazy { SecretClipboard(this, scope) }

    private val lockEvents = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    /** Each time the keys were locked: what they opened must leave the screen. */
    val locks: SharedFlow<Unit> = lockEvents.asSharedFlow()

    override fun onCreate() {
        super.onCreate()
        I18n.apply(resources.configuration)
        ProcessLifecycleOwner.get().lifecycle.addObserver(AutoLock())
    }

    /** Forgets the cached passphrases: the next entry asks again (or a biometric check). */
    fun lock() {
        if (opened) scope.launch(Dispatchers.IO) { passel.lock() }
        lockEvents.tryEmit(Unit)
    }

    /**
     * Locks once the app spent [AUTO_LOCK_MS] in the background. The timer
     * may not fire while Android freezes the process, so coming back also
     * checks how long it was away.
     */
    private inner class AutoLock : DefaultLifecycleObserver {
        private var leftAt: Long? = null
        private var timer: Job? = null

        override fun onStop(owner: LifecycleOwner) {
            leftAt = SystemClock.elapsedRealtime()
            timer?.cancel()
            timer = scope.launch {
                delay(AUTO_LOCK_MS)
                lock()
            }
        }

        override fun onStart(owner: LifecycleOwner) {
            timer?.cancel()
            timer = null
            val away = leftAt?.let { SystemClock.elapsedRealtime() - it }
            leftAt = null
            if (away != null && away >= AUTO_LOCK_MS) lock()
        }
    }

    companion object {
        /** How long the app stays unlocked in the background. */
        const val AUTO_LOCK_MS = 2 * 60 * 1000L
    }
}
