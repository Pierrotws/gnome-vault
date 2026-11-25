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

/** The remote's HTTPS login: username and personal access token. */
data class RemoteLogin(val host: String, val username: String, val token: String)

/**
 * Keeps the remote's token across restarts, encrypted with an AES key that
 * never leaves the Android Keystore (hardware-backed where available).
 * The Rust side only holds it in memory.
 */
class RemoteSecrets(context: Context) {
    private val prefs = context.getSharedPreferences("remote", Context.MODE_PRIVATE)

    fun save(login: RemoteLogin) {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.doFinal(login.token.toByteArray())
        prefs.edit()
            .putString("host", login.host)
            .putString("username", login.username)
            .putString("iv", Base64.encodeToString(cipher.iv, Base64.NO_WRAP))
            .putString("token", Base64.encodeToString(sealed, Base64.NO_WRAP))
            .apply()
    }

    fun load(): RemoteLogin? {
        val host = prefs.getString("host", null) ?: return null
        val username = prefs.getString("username", null) ?: return null
        val iv = prefs.getString("iv", null) ?: return null
        val sealed = prefs.getString("token", null) ?: return null
        return runCatching {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, Base64.decode(iv, Base64.NO_WRAP)))
            RemoteLogin(host, username, String(cipher.doFinal(Base64.decode(sealed, Base64.NO_WRAP))))
        }.getOrNull()
    }

    fun clear() {
        prefs.edit().clear().apply()
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
    }
}
