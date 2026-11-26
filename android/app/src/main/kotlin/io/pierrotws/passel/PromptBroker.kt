package io.pierrotws.passel

import android.os.Looper
import io.pierrotws.passel.core.KeyInfo
import io.pierrotws.passel.core.PassphrasePrompt
import javax.crypto.Cipher
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.runBlocking

/** What the user gave: a passphrase, and whether to unlock with biometrics next time. */
data class PassphraseAnswer(val passphrase: String, val useBiometrics: Boolean = false)

/** A passphrase the Rust side is waiting for. */
sealed interface PromptRequest {
    val answer: CompletableDeferred<PassphraseAnswer?>

    /**
     * Unlock an existing key with its passphrase; [retry] after a wrong
     * one, [biometricsFailed] when the one kept for biometric unlock no
     * longer works. [offerBiometrics]: show "unlock with biometrics".
     */
    data class Unlock(
        val key: KeyInfo,
        val retry: Boolean,
        val biometricsFailed: Boolean = false,
        val offerBiometrics: Boolean = false,
        override val answer: CompletableDeferred<PassphraseAnswer?> = CompletableDeferred(),
    ) : PromptRequest

    /** Choose the passphrase of a key being created. */
    data class NewKey(
        val userId: String,
        val offerBiometrics: Boolean = false,
        override val answer: CompletableDeferred<PassphraseAnswer?> = CompletableDeferred(),
    ) : PromptRequest

    /**
     * Unlock [key] with a biometric check: once it succeeds, [cipher]
     * opens the passphrase kept for it. A null answer falls back to the
     * passphrase.
     */
    data class Biometric(
        val key: KeyInfo,
        val cipher: Cipher,
        override val answer: CompletableDeferred<PassphraseAnswer?> = CompletableDeferred(),
    ) : PromptRequest
}

/**
 * A passphrase to keep for biometric unlock, once the call that asked for
 * it succeeded (so it is known to be right). [fingerprint] is null for a
 * key being created: the call returns it.
 */
data class Enrollment(val fingerprint: String?, val passphrase: String)

/** Turning on biometric unlock for [fingerprint]: a biometric check seals the passphrase with [cipher]. */
data class EnrollRequest(val fingerprint: String, val passphrase: String, val cipher: Cipher)

/**
 * Bridges Rust's blocking passphrase callbacks to the UI: the calling
 * (background) thread publishes a [PromptRequest] and waits; the UI shows
 * a biometric prompt or a dialog and calls [answer].
 */
class PromptBroker(private val biometrics: BiometricUnlock) : PassphrasePrompt {
    private val pending = MutableStateFlow<PromptRequest?>(null)
    val request: StateFlow<PromptRequest?> = pending.asStateFlow()

    private val enrolling = MutableStateFlow<EnrollRequest?>(null)
    /** A biometric check to show, to turn on biometric unlock. */
    val enrollment: StateFlow<EnrollRequest?> = enrolling.asStateFlow()

    @Volatile private var toEnroll: Enrollment? = null

    /** The key last unlocked with its kept passphrase: a retry means it went stale. */
    @Volatile private var unlockedByBiometrics: String? = null

    override fun passphrase(key: KeyInfo, retry: Boolean): String? {
        val fingerprint = key.fingerprint
        var biometricsFailed = false
        if (retry) {
            // The passphrase just given was wrong: do not keep it.
            if (toEnroll?.fingerprint == fingerprint) toEnroll = null
            if (unlockedByBiometrics == fingerprint) {
                // Changed since it was kept (e.g. on a computer).
                unlockedByBiometrics = null
                biometrics.forget(fingerprint)
                biometricsFailed = true
            }
        } else {
            unlockedByBiometrics = null
            biometrics.openingCipher(fingerprint)?.let { cipher ->
                ask(PromptRequest.Biometric(key, cipher))?.let {
                    unlockedByBiometrics = fingerprint
                    return it.passphrase
                }
            }
        }
        val answer = ask(
            PromptRequest.Unlock(
                key,
                retry = retry && !biometricsFailed,
                biometricsFailed = biometricsFailed,
                offerBiometrics = biometrics.available() && !biometrics.isEnrolled(fingerprint),
            ),
        ) ?: return null
        if (answer.useBiometrics) toEnroll = Enrollment(fingerprint, answer.passphrase)
        return answer.passphrase
    }

    override fun newPassphrase(userId: String): String? {
        val answer = ask(PromptRequest.NewKey(userId, offerBiometrics = biometrics.available())) ?: return null
        if (answer.useBiometrics && answer.passphrase.isNotEmpty()) toEnroll = Enrollment(null, answer.passphrase)
        return answer.passphrase
    }

    /** Answers [request]; `null` cancels (or, for a biometric check, asks the passphrase instead). */
    fun answer(request: PromptRequest, answer: PassphraseAnswer?) {
        request.answer.complete(answer)
        pending.compareAndSet(request, null)
    }

    /** The biometric check of [request] ended: [cipher] if it succeeded, else null (ask the passphrase). */
    fun answerBiometric(request: PromptRequest.Biometric, cipher: Cipher?) {
        val passphrase = cipher?.let { biometrics.open(request.key.fingerprint, it) }
        answer(request, passphrase?.let(::PassphraseAnswer))
    }

    /**
     * After a Passel call: when it succeeded, asks for the biometric check
     * that keeps the passphrase it was given ([created]: the fingerprint of
     * the key it created); when it failed, forgets that passphrase.
     */
    fun finishCall(succeeded: Boolean, created: String? = null) {
        val enrollment = toEnroll ?: return
        toEnroll = null
        if (!succeeded) return
        val fingerprint = enrollment.fingerprint ?: created ?: return
        val cipher = biometrics.sealingCipher() ?: return
        enrolling.value = EnrollRequest(fingerprint, enrollment.passphrase, cipher)
    }

    /** The biometric check of [request] succeeded ([cipher] is null if it did not). */
    fun finishEnrollment(request: EnrollRequest, cipher: Cipher?): Boolean {
        enrolling.compareAndSet(request, null)
        cipher ?: return false
        return runCatching { biometrics.seal(request.fingerprint, cipher, request.passphrase) }.isSuccess
    }

    private fun ask(request: PromptRequest): PassphraseAnswer? {
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
