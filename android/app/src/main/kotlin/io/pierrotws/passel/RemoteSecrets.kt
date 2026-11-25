package io.pierrotws.passel

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** An HTTPS login for a remote's host: username and personal access token. */
data class RemoteLogin(val host: String, val username: String, val token: String)

/**
 * Keeps the remotes' tokens across restarts (one per host), encrypted
 * with an AES key that never leaves the Android Keystore (hardware-backed
 * where available). The Rust side only holds them in memory.
 */
class RemoteSecrets(context: Context) {
    private val prefs = context.getSharedPreferences("remote-logins", Context.MODE_PRIVATE)

    fun save(login: RemoteLogin) {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.doFinal(login.token.toByteArray())
        val value = listOf(login.username, cipher.iv, sealed).joinToString(SEPARATOR) {
            when (it) {
                is ByteArray -> Base64.encodeToString(it, Base64.NO_WRAP)
                else -> Base64.encodeToString(it.toString().toByteArray(), Base64.NO_WRAP)
            }
        }
        prefs.edit().putString(login.host, value).apply()
    }

    /** Every saved login; one that cannot be decrypted is skipped. */
    fun loadAll(): List<RemoteLogin> = prefs.all.mapNotNull { (host, value) ->
        val parts = (value as? String)?.split(SEPARATOR)?.takeIf { it.size == 3 } ?: return@mapNotNull null
        runCatching {
            val (username, iv, sealed) = parts.map { Base64.decode(it, Base64.NO_WRAP) }
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
            RemoteLogin(host, String(username), String(cipher.doFinal(sealed)))
        }.getOrNull()
    }

    fun forget(host: String) {
        prefs.edit().remove(host).apply()
    }

    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "passel-remote-token"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val SEPARATOR = ":"
    }
}
