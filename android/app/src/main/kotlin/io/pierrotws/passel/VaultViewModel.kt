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
import io.pierrotws.passel.core.RemoteState
import io.pierrotws.passel.core.VaultInfo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The result of checking a remote address, for the address checked. */
data class RemoteCheckResult(val url: String, val state: RemoteState?)

/** An entry open on screen. */
data class OpenEntry(val ref: EntryRef, val entry: Entry)

data class VaultState(
    /** Opening the keyring and vault (first screen). */
    val starting: Boolean = true,
    val keys: List<KeyInfo> = emptyList(),
    val vaults: List<VaultInfo> = emptyList(),
    /** The setup wizard is open (first run, or "Add a vault"). */
    val addingVault: Boolean = false,
    /** What the wizard's "Check" found at the address it was given. */
    val remoteCheck: RemoteCheckResult? = null,
    val entries: List<EntryRef> = emptyList(),
    val query: String = "",
    /** Local changes not on the remote yet. */
    val unsynced: Int = 0,
    /** What is running, for a progress indicator; null when idle. */
    val busy: String? = null,
    val open: OpenEntry? = null,
    val creating: Boolean = false,
) {
    val activeVault: VaultInfo? get() = vaults.firstOrNull { it.active }
    val hasVault: Boolean get() = activeVault != null

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
                app.remoteSecrets.loadAll().forEach { passel.setHttpsCredentials(it.host, it.username, it.token) }
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

    fun startAddingVault() = mutableState.update { it.copy(addingVault = true, remoteCheck = null, open = null) }

    fun stopAddingVault() = mutableState.update { it.copy(addingVault = false, remoteCheck = null) }

    /** Checks what is at [url] (with this login), for the wizard. */
    fun checkRemote(url: String, username: String, token: String) {
        val address = url.trim()
        mutableState.update { it.copy(remoteCheck = RemoteCheckResult(address, null)) }
        viewModelScope.launch {
            val found = withContext(Dispatchers.IO) {
                useLogin(address, username, token)
                passel.checkRemote(address)
            }
            mutableState.update {
                if (it.remoteCheck?.url == address) it.copy(remoteCheck = RemoteCheckResult(address, found)) else it
            }
        }
    }

    fun cloneVault(name: String, url: String, username: String, token: String, author: String, email: String) =
        run("Downloading the vault") {
            val login = useLogin(url.trim(), username, token)
            val vault = passel.cloneVault(name.trim(), url.trim(), author.trim(), email.trim())
            login?.let(app.remoteSecrets::save)
            mutableState.update { it.copy(addingVault = false, remoteCheck = null) }
            reload()
            say("${vault.name} is ready: ${state.value.entries.size} entries")
        }

    /** [url] is null to keep the vault on this phone only. */
    fun createVault(
        name: String,
        fingerprint: String,
        url: String?,
        username: String,
        token: String,
        author: String,
        email: String,
    ) = run("Creating the vault") {
        val login = url?.let { useLogin(it.trim(), username, token) }
        val created = passel.createVault(name.trim(), fingerprint, url?.trim(), author.trim(), email.trim())
        login?.let(app.remoteSecrets::save)
        mutableState.update { it.copy(addingVault = false, remoteCheck = null) }
        reload()
        say(
            created.uploadError?.let { "${created.vault.name} is ready on this phone. Not sent yet: $it" }
                ?: "${created.vault.name} is ready",
        )
    }

    fun switchVault(id: String) = run(null) {
        passel.switchVault(id)
        mutableState.update { it.copy(open = null, creating = false, query = "") }
        reload()
    }

    fun removeVault(vault: VaultInfo) = run("Removing the vault") {
        passel.removeVault(vault.id)
        mutableState.update { it.copy(open = null, creating = false, query = "") }
        reload()
        // Forget the token when no other vault uses its host.
        val host = vault.remoteUrl?.let { Uri.parse(it).host } ?: return@run
        if (state.value.vaults.none { other -> other.remoteUrl?.let { Uri.parse(it).host } == host }) {
            passel.forgetCredentials(host)
            app.remoteSecrets.forget(host)
        }
    }

    /** Gives Rust the login for [url]'s host; returns it (to save once it worked). */
    private fun useLogin(url: String, username: String, token: String): RemoteLogin? {
        val host = Uri.parse(url).host ?: return null
        if (token.isEmpty()) return null
        return RemoteLogin(host, username.trim(), token).also {
            passel.setHttpsCredentials(it.host, it.username, it.token)
        }
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
        val vaults = passel.vaults()
        val hasVault = vaults.any { it.active }
        val entries = if (hasVault) passel.entries() else emptyList()
        val unsynced = if (hasVault) passel.unsyncedCount().toInt() else 0
        mutableState.update { it.copy(keys = keys, vaults = vaults, entries = entries, unsynced = unsynced) }
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
