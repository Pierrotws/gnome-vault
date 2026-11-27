package io.pierrotws.passel

import android.app.Application
import android.app.PendingIntent
import android.content.Intent
import android.net.Uri
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.pierrotws.passel.core.Entry
import io.pierrotws.passel.core.EntryRef
import io.pierrotws.passel.core.KeyInfo
import io.pierrotws.passel.core.Passel
import io.pierrotws.passel.core.PasselException
import io.pierrotws.passel.core.RemoteState
import io.pierrotws.passel.core.Storage
import io.pierrotws.passel.core.SyncResult
import io.pierrotws.passel.core.VaultInfo
import java.io.File
import java.io.IOException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
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
    /** Some key unlocks with biometrics. */
    val biometricUnlock: Boolean = false,
    /** A key just created here, to offer saving a backup of. */
    val keyToBackUp: String? = null,
    /** Google asks the user to pick an account or allow Drive access: show it. */
    val driveConsent: PendingIntent? = null,
    /** The wizard's Google Drive is connected: the vaults found there. */
    val driveVaults: List<DriveFile>? = null,
    /** Google needs the user to sign in (no access given yet): show the button. */
    val driveSignIn: Boolean = false,
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
        // Locked (by hand or after a while in the background): hide what was decrypted.
        viewModelScope.launch { app.locks.collect { close() } }
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

    fun importKey(uri: Uri) = run(tr("Importing the key")) {
        val bytes = app.contentResolver.openInputStream(uri)?.use { it.readBytes() }
            ?: throw PasselException.Failed(tr("Could not read this file"))
        try {
            passel.importKey(bytes)
        } finally {
            bytes.fill(0)
        }
        reload()
        say(tr("Key imported"))
    }

    fun createKey(name: String, email: String) = run(tr("Creating the key")) {
        val fingerprint = passel.createKey(name, email)
        app.prompts.finishCall(succeeded = true, created = fingerprint)
        reload()
        mutableState.update { it.copy(keyToBackUp = fingerprint) }
    }

    /** Writes the backup of a key (still protected by its passphrase) to [uri]. */
    fun saveKeyBackup(fingerprint: String, uri: Uri) = run(tr("Saving the backup")) {
        val armored = passel.exportKey(fingerprint)
        try {
            val written = try {
                app.contentResolver.openOutputStream(uri, "wt")?.use { it.write(armored) } != null
            } catch (_: java.io.IOException) {
                false
            } catch (_: SecurityException) {
                false
            }
            if (!written) throw PasselException.Failed(tr("Could not write this file"))
        } finally {
            armored.fill(0)
        }
        mutableState.update { it.copy(keyToBackUp = null) }
        say(tr("Backup saved. Keep it safe: with its passphrase, it opens your vaults."))
    }

    fun skipKeyBackup() = mutableState.update { it.copy(keyToBackUp = null) }

    /** The armored public key, for sharing; null if it failed. */
    suspend fun publicKey(fingerprint: String): String? = withContext(Dispatchers.IO) {
        runCatching { String(passel.exportPublicKey(fingerprint)) }.getOrNull()
    }

    fun lock() {
        app.lock()
        say(
            if (state.value.biometricUnlock) {
                tr("Locked: biometrics or the passphrase will be asked again")
            } else {
                tr("Locked: the passphrase will be asked again")
            },
        )
    }

    /** The biometric check that turns on biometric unlock ended ([cipher]: null if it failed). */
    fun finishEnrollment(request: EnrollRequest, cipher: javax.crypto.Cipher?) {
        if (app.prompts.finishEnrollment(request, cipher)) {
            mutableState.update { it.copy(biometricUnlock = true) }
            say(tr("Biometric unlock is on"))
        }
    }

    fun turnOffBiometrics() {
        app.biometrics.forgetAll()
        mutableState.update { it.copy(biometricUnlock = false) }
        say(tr("Biometric unlock is off: the passphrase will be asked"))
    }

    // ---- Vault -----------------------------------------------------------

    fun startAddingVault() =
        mutableState.update { it.copy(addingVault = true, remoteCheck = null, driveVaults = null, driveSignIn = false, open = null) }

    fun stopAddingVault() =
        mutableState.update { it.copy(addingVault = false, remoteCheck = null, driveVaults = null, driveSignIn = false) }

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
        run(tr("Downloading the vault")) {
            val login = useLogin(url.trim(), username, token)
            val vault = passel.cloneVault(name.trim(), url.trim(), author.trim(), email.trim())
            login?.let(app.remoteSecrets::save)
            mutableState.update { it.copy(addingVault = false, remoteCheck = null) }
            reload()
            val count = state.value.entries.size
            say(trn("{name} is ready: {n} entry", "{name} is ready: {n} entries", count, "name" to vault.name, "n" to count))
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
    ) = run(tr("Creating the vault")) {
        val login = url?.let { useLogin(it.trim(), username, token) }
        val created = passel.createVault(name.trim(), fingerprint, url?.trim(), author.trim(), email.trim())
        login?.let(app.remoteSecrets::save)
        mutableState.update { it.copy(addingVault = false, remoteCheck = null) }
        reload()
        say(
            created.uploadError?.let {
                tr("{name} is ready on this phone. Not sent yet: {error}", "name" to created.vault.name, "error" to it)
            } ?: tr("{name} is ready", "name" to created.vault.name),
        )
    }

    fun switchVault(id: String) = run(null) {
        passel.switchVault(id)
        mutableState.update { it.copy(open = null, creating = false, query = "") }
        reload()
    }

    fun removeVault(vault: VaultInfo) = run(tr("Removing the vault")) {
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

    fun sync(quiet: Boolean = false) = run(if (quiet) null else tr("Syncing")) {
        val result = syncActive(interactive = !quiet)
        reload()
        if (!quiet || result.received > 0u) {
            say(
                when {
                    result.received == 0u && result.sent == 0u -> tr("Up to date")
                    else -> tr(
                        "Synced: {received} received, {sent} sent",
                        "received" to result.received,
                        "sent" to result.sent,
                    )
                },
            )
        }
        keptCopiesMessage(result)?.let(::say)
    }

    /** Syncs the open vault with its remote or Google Drive. */
    private suspend fun syncActive(interactive: Boolean): SyncResult =
        when (passel.vaults().firstOrNull { it.active }?.storage) {
            Storage.GOOGLE_DRIVE -> driveSync(interactive)
            else -> passel.sync()
        }

    private fun keptCopiesMessage(result: SyncResult): String? =
        result.keptCopies.takeIf { it.isNotEmpty() }?.let { copies ->
            tr(
                "Changed both here and on Google Drive: this phone's version is kept as {entries}. Check it, then delete the one you do not need.",
                "entries" to copies.joinToString(),
            )
        }

    // ---- Google Drive -------------------------------------------------------

    private var consentAnswer: CompletableDeferred<String?>? = null

    private var driveCheck: Job? = null

    /**
     * Connects the wizard to Google Drive without asking when Google already
     * gave access; otherwise (or when it fails) asks for the sign-in button.
     */
    fun checkDrive() {
        if (driveCheck?.isActive == true) return
        driveCheck = viewModelScope.launch(Dispatchers.IO) {
            val vaults = try {
                withDrive(interactive = false) { it.vaults() }
            } catch (e: Exception) {
                Log.i("Passel", "Google Drive is not connected yet: ${e.message}")
                null
            }
            mutableState.update { if (vaults != null) it.copy(driveVaults = vaults) else it.copy(driveSignIn = true) }
        }
    }

    /** Signs in to Google Drive (the user tapped the button) and lists the vaults there. */
    fun connectDrive() = run(tr("Connecting to Google Drive")) {
        val vaults = withDrive(interactive = true) { it.vaults() }
        mutableState.update { it.copy(driveVaults = vaults, driveSignIn = false) }
    }

    /** The consent screen of [VaultState.driveConsent] is showing. */
    fun driveConsentShown() = mutableState.update { it.copy(driveConsent = null) }

    /** The consent screen ended ([data]: its result). */
    fun finishDriveConsent(data: Intent?) {
        consentAnswer?.complete(app.driveAuth.tokenFromConsent(data))
    }

    fun downloadDriveVault(file: DriveFile, name: String, author: String, email: String) =
        run(tr("Downloading the vault")) {
            val vault = withDrive(interactive = true) { drive ->
                withArchiveFile { archive ->
                    drive.download(file.id, archive)
                    passel.downloadDriveVault(name.trim(), archive.path, file.id, file.version, author.trim(), email.trim())
                }
            }
            mutableState.update { it.copy(addingVault = false, driveVaults = null) }
            reload()
            val count = state.value.entries.size
            say(trn("{name} is ready: {n} entry", "{name} is ready: {n} entries", count, "name" to vault.name, "n" to count))
        }

    fun createDriveVault(name: String, fingerprint: String, author: String, email: String) =
        run(tr("Creating the vault")) {
            val vault = passel.createDriveVault(name.trim(), fingerprint, author.trim(), email.trim())
            mutableState.update { it.copy(addingVault = false, driveVaults = null) }
            reload()
            say(
                try {
                    driveSync(interactive = true)
                    tr("{name} is ready", "name" to vault.name)
                } catch (e: PasselException) {
                    tr("{name} is ready on this phone. Not sent yet: {error}", "name" to vault.name, "error" to describe(e))
                },
            )
            reload()
        }

    /**
     * Exchanges the open vault with its Drive file: downloads and merges
     * it when it changed, then uploads this vault when Drive lacks some of
     * its changes. Another device uploading in between is caught by
     * checking the file's version just before uploading.
     */
    private suspend fun driveSync(interactive: Boolean): SyncResult = withDrive(interactive) { drive ->
        var received = 0u
        var keptCopies = emptyList<String>()
        repeat(DRIVE_ATTEMPTS) {
            val vault = passel.vaults().first { it.active }
            // Null for a vault not uploaded yet, or whose file was deleted
            // from Drive: it gets a new one.
            val current = vault.driveFileId?.let(drive::file)
            if (current != null && current.version != vault.driveVersion) {
                val merged = withArchiveFile { archive ->
                    drive.download(current.id, archive)
                    passel.mergeDriveArchive(archive.path, current.version)
                }
                received += merged.received
                keptCopies = keptCopies + merged.keptCopies
            }
            val unsent = passel.unsyncedCount()
            if (current != null && unsent == 0u) return@withDrive SyncResult(received, 0u, keptCopies)
            val uploaded = withArchiveFile { archive ->
                passel.packVault(archive.path)
                when {
                    current == null -> drive.create(vault.name, archive)
                    // Changed meanwhile: merge that first.
                    drive.file(current.id)?.version != current.version -> null
                    else -> drive.update(current.id, archive)
                }
            } ?: return@repeat
            passel.driveUploaded(uploaded.id, uploaded.version)
            return@withDrive SyncResult(received, unsent, keptCopies)
        }
        throw PasselException.Failed(tr("Google Drive changed during the sync: sync again"))
    }

    /**
     * Runs [block] with a Drive client. With [interactive], the user may be
     * asked to choose an account or allow access; otherwise that fails.
     * An expired token is renewed once.
     */
    private suspend fun <T> withDrive(interactive: Boolean, block: (DriveClient) -> T): T {
        var token = driveToken(interactive)
        repeat(2) { attempt ->
            try {
                return block(DriveClient(token))
            } catch (e: DriveException) {
                if (e.code != 401 || attempt > 0) throw driveFailure(e)
                app.driveAuth.clearToken(token)
                token = driveToken(interactive)
            } catch (e: IOException) {
                throw driveFailure(e)
            }
        }
        error("unreachable")
    }

    private suspend fun driveToken(interactive: Boolean): String {
        val result = try {
            app.driveAuth.authorize()
        } catch (e: Exception) {
            Log.i("Passel", "Google authorization failed: ${e.javaClass.simpleName} ${e.message}")
            throw PasselException.Failed(tr("Could not connect to Google Drive"))
        }
        return when (result) {
            is DriveAuth.Result.Token -> result.token
            is DriveAuth.Result.NeedsConsent -> {
                if (!interactive) throw PasselException.Failed(tr("Tap Sync to sign in to Google Drive again"))
                val answer = CompletableDeferred<String?>()
                consentAnswer = answer
                mutableState.update { it.copy(driveConsent = result.intent) }
                try {
                    answer.await() ?: throw PasselException.Cancelled()
                } finally {
                    consentAnswer = null
                    mutableState.update { it.copy(driveConsent = null) }
                }
            }
        }
    }

    private fun driveFailure(e: IOException): PasselException = when {
        e is DriveException && (e.code == 401 || e.code == 403) -> PasselException.Failed(tr("Google Drive refused access"))
        e is DriveException && e.code == 507 -> PasselException.Failed(tr("Your Google Drive is full"))
        else -> PasselException.Failed(tr("Could not reach Google Drive"))
    }

    /** A temporary file for an archive, deleted after [block]. */
    private fun <T> withArchiveFile(block: (File) -> T): T {
        val file = File.createTempFile("vault", DriveClient.SUFFIX, app.cacheDir)
        try {
            return block(file)
        } finally {
            file.delete()
        }
    }

    // ---- Entries -----------------------------------------------------------

    fun open(ref: EntryRef) = run(tr("Decrypting")) {
        val entry = passel.openEntry(ref.path)
        mutableState.update { it.copy(open = OpenEntry(ref, entry)) }
    }

    /** [onSaved] runs (on the main thread) once the entry is written. */
    fun save(ref: EntryRef, entry: Entry, onSaved: () -> Unit) = run(tr("Saving")) {
        passel.saveEntry(ref.path, entry)
        mutableState.update { it.copy(open = OpenEntry(ref, entry)) }
        viewModelScope.launch { onSaved() }
        syncAfterChange()
    }

    fun create(folder: String, name: String, entry: Entry) = run(tr("Saving")) {
        val path = passel.createEntry(folder.trim().trim('/'), name.trim(), entry)
        reload()
        val ref = state.value.entries.first { it.path == path }
        mutableState.update { it.copy(creating = false, open = OpenEntry(ref, entry)) }
        syncAfterChange()
    }

    fun delete(ref: EntryRef) = run(tr("Deleting")) {
        passel.deleteEntry(ref.path)
        mutableState.update { it.copy(open = null) }
        reload()
        syncAfterChange()
    }

    fun copy(text: String, what: String) {
        if (app.clipboard.copy(text)) say(
                tr(
                    "{what} copied; cleared in {seconds} s",
                    "what" to what,
                    "seconds" to SecretClipboard.CLEAR_AFTER_SECONDS,
                ),
            )
    }

    // ---- Plumbing -----------------------------------------------------------

    /** Sends a change right away; offline, it stays committed here. */
    private suspend fun syncAfterChange() {
        try {
            syncActive(interactive = false).let { result -> keptCopiesMessage(result)?.let(::say) }
            reload()
        } catch (e: PasselException) {
            reload()
            say(tr("Saved on this phone. Not synced: {error}", "error" to describe(e)))
        }
    }

    private fun reload() {
        val keys = passel.keys()
        val vaults = passel.vaults()
        val hasVault = vaults.any { it.active }
        val entries = if (hasVault) passel.entries() else emptyList()
        val unsynced = if (hasVault) passel.unsyncedCount().toInt() else 0
        val biometricUnlock = app.biometrics.anyEnrolled()
        mutableState.update {
            it.copy(keys = keys, vaults = vaults, entries = entries, unsynced = unsynced, biometricUnlock = biometricUnlock)
        }
    }

    private fun run(label: String?, block: suspend () -> Unit) {
        viewModelScope.launch {
            mutableState.update { it.copy(busy = label) }
            var succeeded = false
            try {
                withContext(Dispatchers.IO) { block() }
                succeeded = true
            } catch (e: PasselException) {
                if (e !is PasselException.Cancelled) say(describe(e))
            } finally {
                // Keep a passphrase for biometric unlock only once it worked.
                app.prompts.finishCall(succeeded)
                mutableState.update { it.copy(busy = null) }
            }
        }
    }

    private fun say(message: String) {
        messageChannel.trySend(message)
    }

    private fun describe(e: PasselException): String = when (e) {
        is PasselException.Cancelled -> tr("Cancelled")
        is PasselException.AccessDenied -> tr("The remote refused your username or token")
        is PasselException.Conflict -> e.keptAs?.let { branch ->
            tr(
                "{entries} changed here and on another device. Your version was sent as branch {branch}: merge it on a computer, then sync here. Nothing was lost.",
                "entries" to e.entries.joinToString(),
                "branch" to branch,
            )
        } ?: tr(
            "{entries} changed here and on another device, and your version could not be sent. It is kept on this phone; nothing was lost.",
            "entries" to e.entries.joinToString(),
        )
        is PasselException.Failed -> e.reason
    }

    private companion object {
        /** Syncs retried when another device uploads at the same time. */
        const val DRIVE_ATTEMPTS = 3
    }
}
