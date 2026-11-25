package io.pierrotws.passel

import android.os.Looper
import io.pierrotws.passel.core.KeyInfo
import io.pierrotws.passel.core.PassphrasePrompt
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.runBlocking

/** A passphrase the Rust side is waiting for. */
sealed interface PromptRequest {
    val answer: CompletableDeferred<String?>

    /** Unlock an existing key; [retry] after a wrong passphrase. */
    data class Unlock(
        val key: KeyInfo,
        val retry: Boolean,
        override val answer: CompletableDeferred<String?> = CompletableDeferred(),
    ) : PromptRequest

    /** Choose the passphrase of a key being created. */
    data class NewKey(
        val userId: String,
        override val answer: CompletableDeferred<String?> = CompletableDeferred(),
    ) : PromptRequest
}

/**
 * Bridges Rust's blocking passphrase callbacks to the UI: the calling
 * (background) thread publishes a [PromptRequest] and waits; the UI shows
 * a dialog and calls [answer].
 */
class PromptBroker : PassphrasePrompt {
    private val pending = MutableStateFlow<PromptRequest?>(null)
    val request: StateFlow<PromptRequest?> = pending.asStateFlow()

    override fun passphrase(key: KeyInfo, retry: Boolean): String? = ask(PromptRequest.Unlock(key, retry))

    override fun newPassphrase(userId: String): String? = ask(PromptRequest.NewKey(userId))

    /** Answers [request]; `null` cancels. */
    fun answer(request: PromptRequest, passphrase: String?) {
        request.answer.complete(passphrase)
        pending.compareAndSet(request, null)
    }

    private fun ask(request: PromptRequest): String? {
        check(Looper.myLooper() != Looper.getMainLooper()) {
            "Passel calls must run off the main thread: the prompt would deadlock"
        }
        pending.value = request
        return try {
            runBlocking { request.answer.await() }
        } finally {
            pending.compareAndSet(request, null)
        }
    }
}
