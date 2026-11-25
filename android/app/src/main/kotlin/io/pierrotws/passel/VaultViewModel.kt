package io.pierrotws.passel

import android.app.Application
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.pierrotws.passel.core.Entry
import io.pierrotws.passel.core.EntryRef
import io.pierrotws.passel.core.KeyInfo
import io.pierrotws.passel.core.Passel
import io.pierrotws.passel.core.PasselException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** An entry open on screen. */
data class OpenEntry(val ref: EntryRef, val entry: Entry)

data class VaultState(
    /** Opening the keyring and vault (first screen). */
    val starting: Boolean = true,
    val keys: List<KeyInfo> = emptyList(),
    val hasVault: Boolean = false,
    val entries: List<EntryRef> = emptyList(),
    val query: String = "",
    /** Local changes not on the remote yet. */
    val unsynced: Int = 0,
    /** What is running, for a progress indicator; null when idle. */
    val busy: String? = null,
    val open: OpenEntry? = null,
    val creating: Boolean = false,
) {
    val filtered: List<EntryRef>
        get() = if (query.isBlank()) entries else entries.filter { it.path.contains(query.trim(), ignoreCase = true) }
}

/**
 * The app's state and actions. Every Passel call blocks (and may wait for
 * a passphrase), so all of them run on [Dispatchers.IO].
 */
class VaultViewModel(application: Application) : AndroidViewModel(application) {
    private val app = application as PasselApp
    private val passel: Passel get() = app.passel

    private val mutableState = MutableStateFlow(VaultState())
    val state: StateFlow<VaultState> = mutableState.asStateFlow()

    private val messageChannel = Channel<String>(Channel.BUFFERED)
    /** One-off messages for a snackbar. */
    val messages = messageChannel.receiveAsFlow()

    init {
        viewModelScope.launch {
            withContext(Dispatchers.IO) {
                app.remoteSecrets.load()?.let { passel.setHttpsCredentials(it.host, it.username, it.token) }
            }
            reload()
            mutableState.update { it.copy(starting = false) }
            if (state.value.hasVault) sync(quiet = true)
        }
    }

    fun setQuery(query: String) = mutableState.update { it.copy(query = query) }

    fun startCreating() = mutableState.update { it.copy(creating = true) }

    fun close() = mutableState.update { it.copy(open = null, creating = false) }

    // ---- Keys ----------------------------------------------------------

    fun importKey(uri: Uri) = run("Importing the key") {
        val bytes = app.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            ?: throw PasselException.Failed("Could not read this file")
        try {
            passel.importKey(bytes)
        } finally {
            bytes.fill(0)
        }
        reload()
        say("Key imported")
    }

    fun createKey(name: String, email: String) = run("Creating the key") {
        passel.createKey(name, email)
        reload()
        say("Key created. Add its public key to your vault from a computer.")
    }

    /** The armored public key, for sharing; null if it failed. */
    suspend fun publicKey(fingerprint: String): String? = withContext(Dispatchers.IO) {
        runCatching { String(passel.exportPublicKey(fingerprint)) }.getOrNull()
    }

    fun lock() {
        viewModelScope.launch(Dispatchers.IO) { passel.lock() }
        close()
        say("Locked: the passphrase will be asked again")
    }

    // ---- Vault -----------------------------------------------------------

    fun cloneVault(url: String, username: String, token: String, name: String, email: String) =
        run("Downloading the vault") {
            val host = Uri.parse(url.trim()).host
            if (host != null && token.isNotEmpty()) {
                val login = RemoteLogin(host, username.trim(), token)
                passel.setHttpsCredentials(login.host, login.username, login.token)
                app.remoteSecrets.save(login)
            }
            passel.cloneVault(url.trim(), name.trim(), email.trim())
            reload()
            say("Vault ready: ${state.value.entries.size} entries")
        }

    fun removeVault() = run("Removing the vault") {
        passel.removeVault()
        app.remoteSecrets.load()?.let { passel.forgetCredentials(it.host) }
        app.remoteSecrets.clear()
        mutableState.update { it.copy(open = null, creating = false) }
        reload()
    }

    fun sync(quiet: Boolean = false) = run(if (quiet) null else "Syncing") {
        val result = passel.sync()
        reload()
        if (!quiet || result.received > 0u) {
            say(
                when {
                    result.received == 0u && result.sent == 0u -> "Up to date"
                    else -> "Synced: ${result.received} received, ${result.sent} sent"
                },
            )
        }
    }

    // ---- Entries -----------------------------------------------------------

    fun open(ref: EntryRef) = run("Decrypting") {
        val entry = passel.openEntry(ref.path)
        mutableState.update { it.copy(open = OpenEntry(ref, entry)) }
    }

    /** [onSaved] runs (on the main thread) once the entry is written. */
    fun save(ref: EntryRef, entry: Entry, onSaved: () -> Unit) = run("Saving") {
        passel.saveEntry(ref.path, entry)
        mutableState.update { it.copy(open = OpenEntry(ref, entry)) }
        viewModelScope.launch { onSaved() }
        syncAfterChange()
    }

    fun create(folder: String, name: String, entry: Entry) = run("Saving") {
        val path = passel.createEntry(folder.trim().trim('/'), name.trim(), entry)
        reload()
        val ref = state.value.entries.first { it.path == path }
        mutableState.update { it.copy(creating = false, open = OpenEntry(ref, entry)) }
        syncAfterChange()
    }

    fun delete(ref: EntryRef) = run("Deleting") {
        passel.deleteEntry(ref.path)
        mutableState.update { it.copy(open = null) }
        reload()
        syncAfterChange()
    }

    fun copy(text: String, what: String) {
        if (app.clipboard.copy(text)) say("$what copied; cleared in ${SecretClipboard.CLEAR_AFTER_SECONDS} s")
    }

    // ---- Plumbing -----------------------------------------------------------

    /** Sends a change right away; offline, it stays committed here. */
    private fun syncAfterChange() {
        try {
            passel.sync()
            reload()
        } catch (e: PasselException) {
            reload()
            say("Saved on this phone. Not synced: ${describe(e)}")
        }
    }

    private fun reload() {
        val keys = passel.keys()
        val hasVault = passel.hasVault()
        val entries = if (hasVault) passel.entries() else emptyList()
        val unsynced = if (hasVault) passel.unsyncedCount().toInt() else 0
        mutableState.update { it.copy(keys = keys, hasVault = hasVault, entries = entries, unsynced = unsynced) }
    }

    private fun run(label: String?, block: () -> Unit) {
        viewModelScope.launch {
            mutableState.update { it.copy(busy = label) }
            try {
                withContext(Dispatchers.IO) { block() }
            } catch (e: PasselException) {
                if (e !is PasselException.Cancelled) say(describe(e))
            } finally {
                mutableState.update { it.copy(busy = null) }
            }
        }
    }

    private fun say(message: String) {
        messageChannel.trySend(message)
    }

    private fun describe(e: PasselException): String = when (e) {
        is PasselException.Cancelled -> "Cancelled"
        is PasselException.AccessDenied -> "The remote refused your username or token"
        is PasselException.Conflict -> e.keptAs?.let { branch ->
            "${e.entries.joinToString()} changed here and on another device. Your version was " +
                "sent as branch $branch: merge it on a computer, then sync here. Nothing was lost."
        } ?: ("${e.entries.joinToString()} changed here and on another device, and your version " +
            "could not be sent. It is kept on this phone; nothing was lost.")
        is PasselException.Failed -> e.reason
    }
}
