package io.pierrotws.passel

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.annotation.RequiresApi
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Keeps key passphrases for biometric unlock: each one is encrypted with
 * an AES key that never leaves the Android Keystore and only works right
 * after a strong biometric check (fingerprint or face, class 3). Every
 * use goes through a `BiometricPrompt` given the [Cipher] made here.
 *
 * Enrolling a new fingerprint or face, or removing the screen lock,
 * invalidates the Keystore key: the stored passphrases are then dropped
 * and the passphrase is asked again.
 */
class BiometricUnlock(private val context: Context) {
    private val prefs = context.getSharedPreferences("biometric-unlock", Context.MODE_PRIVATE)

    /**
     * Whether this phone can do it: Android 11+ (earlier versions may fall
     * back to a fingerprint dialog this app's theme cannot show), with a
     * strong biometric enrolled.
     */
    fun available(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
            BiometricManager.from(context).canAuthenticate(BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS

    fun isEnrolled(fingerprint: String): Boolean = prefs.contains(fingerprint)

    fun anyEnrolled(): Boolean = prefs.all.isNotEmpty()

    /** A cipher to seal a passphrase with, once the prompt succeeded; null if unavailable. */
    fun sealingCipher(): Cipher? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R || !available()) return null
        return try {
            Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key()) }
        } catch (_: KeyPermanentlyInvalidatedException) {
            forgetAll()
            runCatching { Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key()) } }.getOrNull()
        }
    }

    /** A cipher to open [fingerprint]'s passphrase with, once the prompt succeeded; null if none. */
    fun openingCipher(fingerprint: String): Cipher? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R || !available()) return null
        val (iv, _) = stored(fingerprint) ?: return null
        return try {
            Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv)) }
        } catch (_: KeyPermanentlyInvalidatedException) {
            forgetAll()
            null
        } catch (_: Exception) {
            forget(fingerprint)
            null
        }
    }

    /** Stores [passphrase] for [fingerprint] with an authenticated [cipher] from [sealingCipher]. */
    fun seal(fingerprint: String, cipher: Cipher, passphrase: String) {
        val sealed = cipher.doFinal(passphrase.toByteArray())
        val value = listOf(cipher.iv, sealed).joinToString(SEPARATOR) { Base64.encodeToString(it, Base64.NO_WRAP) }
        prefs.edit().putString(fingerprint, value).apply()
    }

    /** [fingerprint]'s passphrase, with an authenticated [cipher] from [openingCipher]. */
    fun open(fingerprint: String, cipher: Cipher): String? {
        val (_, sealed) = stored(fingerprint) ?: return null
        return runCatching { String(cipher.doFinal(sealed)) }.getOrNull()
    }

    fun forget(fingerprint: String) {
        prefs.edit().remove(fingerprint).apply()
    }

    /** Drops every stored passphrase and the Keystore key. */
    fun forgetAll() {
        prefs.edit().clear().apply()
        runCatching { KeyStore.getInstance(KEYSTORE).apply { load(null) }.deleteEntry(ALIAS) }
    }

    private fun stored(fingerprint: String): Pair<ByteArray, ByteArray>? {
        val parts = prefs.getString(fingerprint, null)?.split(SEPARATOR)?.takeIf { it.size == 2 } ?: return null
        return runCatching { Base64.decode(parts[0], Base64.NO_WRAP) to Base64.decode(parts[1], Base64.NO_WRAP) }.getOrNull()
    }

    @RequiresApi(Build.VERSION_CODES.R)
    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setUserAuthenticationRequired(true)
                // Every use needs its own biometric check (no time window),
                // and only a strong biometric: not the device's PIN.
                .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
                .setInvalidatedByBiometricEnrollment(true)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "passel-biometric-unlock"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val SEPARATOR = ":"
    }
}
