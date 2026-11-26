package io.pierrotws.passel

import android.app.Application
import io.pierrotws.passel.core.Passel
import kotlinx.coroutines.MainScope

/**
 * Process-wide objects. The Rust side keeps one keyring and one set of
 * credentials per process, so [passel] lives here rather than in an
 * activity.
 */
class PasselApp : Application() {
    /** Outlives activities: clipboard clearing, background syncs. */
    val scope = MainScope()

    val prompts = PromptBroker()

    /** Opens the keyring and vault under `files/passel`. Blocks: first use off the main thread. */
    val passel: Passel by lazy { Passel(filesDir.resolve("passel").path, prompts) }

    val remoteSecrets by lazy { RemoteSecrets(this) }

    val clipboard by lazy { SecretClipboard(this, scope) }

    override fun onCreate() {
        super.onCreate()
        I18n.apply(resources.configuration)
    }
}
