package io.pierrotws.passel

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.pierrotws.passel.core.KeyInfo
import io.pierrotws.passel.core.RemoteState
import kotlinx.coroutines.launch

private enum class Step { WELCOME, KIND, KEY, PLACE, LOCAL, IDENTITY }

private enum class Kind { DOWNLOAD, CREATE }

/** Where the vault is kept, in the order offered. */
private enum class Place { DRIVE, GIT, PHONE }

/**
 * For [Place.PHONE]: the copy kept in a place the user picks. There is
 * always one: the app's own storage goes when its data is cleared or it
 * is uninstalled, the copy stays, and the vault can be downloaded again
 * from it.
 */
private enum class Local { FOLDER, FILE }

/**
 * Sets up a vault, step by step: on first run (from a welcome page) and
 * from "Add a vault". Like the desktop wizard: a new vault or an existing
 * one, then for a new one its key, where it lives, and who signs this
 * device's changes. An existing vault's key is asked for after it is
 * downloaded, and only when this phone does not hold it ([VaultKeyPage]).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SetupWizard(state: VaultState, model: VaultViewModel) {
    state.vaultKeyNeeded?.let {
        VaultKeyPage(it, state, model)
        return
    }
    val firstRun = state.vaults.isEmpty()
    var step by rememberSaveable { mutableStateOf(if (firstRun) Step.WELCOME else Step.KIND) }
    var kind by rememberSaveable { mutableStateOf<Kind?>(null) }
    var fingerprint by rememberSaveable { mutableStateOf<String?>(null) }
    var place by rememberSaveable { mutableStateOf(Place.DRIVE) }
    // The Drive file of the vault to download.
    var driveFileId by rememberSaveable { mutableStateOf<String?>(null) }
    val driveFile = state.driveVaults?.firstOrNull { it.id == driveFileId }
    var url by rememberSaveable { mutableStateOf("") }
    var username by rememberSaveable { mutableStateOf("") }
    // Not saved in the instance state: it is a secret.
    var token by remember { mutableStateOf("") }
    var local by rememberSaveable { mutableStateOf(Local.FOLDER) }
    // The folder or file picked for it, and its name.
    var localUri by rememberSaveable { mutableStateOf<String?>(null) }
    var localName by rememberSaveable { mutableStateOf<String?>(null) }
    var vaultName by rememberSaveable { mutableStateOf("") }
    var author by rememberSaveable { mutableStateOf("") }
    var email by rememberSaveable { mutableStateOf("") }

    // Keep a usable key selected as keys are imported or created.
    LaunchedEffect(state.keys) {
        if (state.keys.none { it.fingerprint == fingerprint }) fingerprint = state.keys.lastOrNull()?.fingerprint
    }
    // Fill the last page from what is known by then.
    LaunchedEffect(step) {
        if (step != Step.IDENTITY) return@LaunchedEffect
        val key = state.keys.firstOrNull { it.fingerprint == fingerprint } ?: state.keys.firstOrNull()
        val (keyName, keyEmail) = splitUserId(key?.userId)
        if (author.isBlank()) author = keyName
        if (email.isBlank()) email = keyEmail
        if (vaultName.isBlank()) {
            vaultName = when (place) {
                Place.DRIVE -> driveFile?.vaultName?.takeIf { kind == Kind.DOWNLOAD }
                Place.GIT -> repositoryName(url)
                Place.PHONE -> localName?.removeSuffix(".bundle")?.takeIf { kind == Kind.DOWNLOAD && local == Local.FILE }
            }
                // Translators: default names of a new vault.
                ?: if (firstRun) tr("Passwords") else tr("Vault")
        }
    }

    val order = listOfNotNull(
        Step.WELCOME.takeIf { firstRun },
        Step.KIND,
        // An existing vault comes with its key: asked for after, if needed.
        Step.KEY.takeIf { kind != Kind.DOWNLOAD },
        Step.PLACE,
        Step.LOCAL.takeIf { place == Place.PHONE },
        Step.IDENTITY,
    )
    // A new kind of vault, or of copy, needs a new pick.
    fun switchKind(to: Kind) {
        if (to != kind) localUri = null
        kind = to
    }
    val index = order.indexOf(step)
    val idle = state.busy == null
    fun back() {
        if (index > 0) step = order[index - 1] else model.stopAddingVault()
    }
    BackHandler(enabled = index > 0 || !firstRun) { back() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (firstRun) tr("Set up Passel") else tr("Add a vault")) },
                navigationIcon = {
                    when {
                        index > 0 -> IconButton(onClick = ::back, enabled = idle) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = tr("Back"))
                        }
                        !firstRun -> IconButton(onClick = model::stopAddingVault) {
                            Icon(Icons.Default.Close, contentDescription = tr("Cancel"))
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .imePadding()
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            LinearProgressIndicator(
                progress = { (index + 1f) / order.size },
                modifier = Modifier.fillMaxWidth(),
            )
            Text(tr("Step {step} of {total}", "step" to index + 1, "total" to order.size), style = MaterialTheme.typography.labelMedium)
            when (step) {
                Step.WELCOME -> WelcomePage(onStart = { step = Step.KIND })
                Step.KIND -> KindPage(kind) {
                    switchKind(it)
                    step = if (it == Kind.CREATE) Step.KEY else Step.PLACE
                }
                Step.KEY -> KeyPage(state, fingerprint, onSelect = { fingerprint = it }, model)
                Step.PLACE -> PlacePage(
                    kind = kind ?: Kind.DOWNLOAD,
                    place = place,
                    onPlace = { place = it },
                    driveFileId = driveFileId,
                    onDriveFile = { driveFileId = it },
                    url = url,
                    onUrl = { url = it },
                    username = username,
                    onUsername = { username = it },
                    token = token,
                    onToken = { token = it },
                    state = state,
                    model = model,
                    onSwitchKind = {
                        switchKind(it)
                        // A new vault needs its key first.
                        if (it == Kind.CREATE) step = Step.KEY
                    },
                )
                Step.LOCAL -> LocalPage(
                    kind = kind ?: Kind.DOWNLOAD,
                    local = local,
                    onLocal = {
                        if (it != local) localUri = null
                        local = it
                    },
                    pickedName = localName.takeIf { localUri != null },
                    onPicked = { uri, name ->
                        localUri = uri
                        localName = name
                    },
                )
                Step.IDENTITY -> IdentityPage(vaultName, { vaultName = it }, author, { author = it }, email, { email = it })
            }
            state.busy?.let {
                Text(it)
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }

            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.End) {
                when (step) {
                    Step.WELCOME, Step.KIND -> {}
                    Step.KEY -> Button(onClick = { step = Step.PLACE }, enabled = idle && fingerprint != null) { Text(tr("Next")) }
                    Step.PLACE -> Button(
                        onClick = { step = order[index + 1] },
                        enabled = idle && when (place) {
                            Place.DRIVE -> if (kind == Kind.CREATE) state.driveVaults != null else driveFile != null
                            Place.GIT -> url.isNotBlank()
                            Place.PHONE -> true
                        },
                    ) { Text(tr("Next")) }
                    Step.LOCAL -> Button(
                        onClick = { step = Step.IDENTITY },
                        enabled = idle && localUri != null,
                    ) { Text(tr("Next")) }
                    Step.IDENTITY -> Button(
                        onClick = {
                            when (kind to place) {
                                Kind.CREATE to Place.DRIVE -> model.createDriveVault(vaultName, fingerprint!!, author, email)
                                Kind.CREATE to Place.GIT -> model.createVault(
                                    vaultName, fingerprint!!, url, username, token, author, email,
                                )
                                Kind.CREATE to Place.PHONE -> localUri?.let(Uri::parse)?.let { picked ->
                                    if (local == Local.FILE) {
                                        model.createFileVault(picked, vaultName, fingerprint!!, author, email)
                                    } else {
                                        model.createFolderVault(picked, vaultName, fingerprint!!, author, email)
                                    }
                                }
                                Kind.DOWNLOAD to Place.PHONE -> localUri?.let(Uri::parse)?.let { picked ->
                                    if (local == Local.FILE) {
                                        model.downloadFileVault(picked, vaultName, author, email)
                                    } else {
                                        model.downloadFolderVault(picked, vaultName, author, email)
                                    }
                                }
                                Kind.DOWNLOAD to Place.DRIVE -> driveFile?.let { model.downloadDriveVault(it, vaultName, author, email) }
                                else -> model.cloneVault(vaultName, url, username, token, author, email)
                            }
                        },
                        enabled = idle && vaultName.isNotBlank() && author.isNotBlank() && email.isNotBlank() &&
                            (kind == Kind.DOWNLOAD || fingerprint != null),
                    ) { Text(if (kind == Kind.CREATE) tr("Create the vault") else tr("Download the vault")) }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun WelcomePage(onStart: () -> Unit) {
    Text(tr("Welcome to Passel"), style = MaterialTheme.typography.headlineMedium)
    Text(
        tr(
            "Passel keeps your passwords in a pass vault: one encrypted file per entry, with its whole history. Keep it on your Google Drive, in a git repository, or in a folder or file of your own.",
        ),
    )
    Text(tr("Setting it up takes a few steps: your vault, then your key."))
    Button(onClick = onStart, modifier = Modifier.fillMaxWidth()) { Text(tr("Get started")) }
}

@Composable
private fun KeyPage(state: VaultState, selected: String?, onSelect: (String) -> Unit, model: VaultViewModel) {
    val pickKey = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(model::importKey)
    }
    var creating by rememberSaveable { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var email by rememberSaveable { mutableStateOf("") }
    val idle = state.busy == null

    PageTitle(tr("Your encryption key"))
    Text(
        tr(
            "Entries are encrypted with an OpenPGP key. To open a vault made on a computer, import the backup of its key (gpg --export-secret-keys --armor <id>). A new vault can use a key created here.",
        ),
    )
    state.keys.forEach { key ->
        Choice(selected = key.fingerprint == selected, onClick = { onSelect(key.fingerprint) }) {
            KeyLine(key, model)
        }
    }
    if (!creating) {
        Text(if (state.keys.isEmpty()) tr("Get a key:") else tr("Or add another key:"), style = MaterialTheme.typography.titleSmall)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { pickKey.launch(arrayOf("*/*")) }, enabled = idle) { Text(tr("Import a key")) }
            OutlinedButton(onClick = { creating = true }, enabled = idle) { Text(tr("Create a key")) }
        }
    } else {
        Text(tr("Create a key"), style = MaterialTheme.typography.titleSmall)
        OutlinedTextField(name, { name = it }, label = { Text(tr("Your name")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(
            email,
            { email = it },
            label = { Text(tr("Email (optional)")) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = { model.createKey(name, email); creating = false }, enabled = idle && name.isNotBlank()) {
                Text(tr("Create"))
            }
            TextButton(onClick = { creating = false }, enabled = idle) { Text(tr("Cancel")) }
        }
    }
}

@Composable
private fun KindPage(selected: Kind?, onSelect: (Kind) -> Unit) {
    PageTitle(tr("Your vault"))
    Choice(selected = selected == Kind.DOWNLOAD, onClick = { onSelect(Kind.DOWNLOAD) }) {
        ChoiceText(
            tr("I already have a vault"),
            tr("Get it from Google Drive, a git repository, or a folder or file: made with Passel, or with pass and git."),
        )
    }
    Choice(selected = selected == Kind.CREATE, onClick = { onSelect(Kind.CREATE) }) {
        ChoiceText(tr("Create a new vault"), tr("Start an empty vault, encrypted with your key."))
    }
}

@Composable
private fun PlacePage(
    kind: Kind,
    place: Place,
    onPlace: (Place) -> Unit,
    driveFileId: String?,
    onDriveFile: (String) -> Unit,
    url: String,
    onUrl: (String) -> Unit,
    username: String,
    onUsername: (String) -> Unit,
    token: String,
    onToken: (String) -> Unit,
    state: VaultState,
    model: VaultViewModel,
    onSwitchKind: (Kind) -> Unit,
) {
    val remoteFields = @Composable {
        OutlinedTextField(
            url,
            onUrl,
            label = { Text(tr("Address (https://…)")) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(username, onUsername, label = { Text(tr("Username")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        PasswordField(token, onToken, tr("Access token"), Modifier.fillMaxWidth())
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { model.checkRemote(url, username, token) }, enabled = url.isNotBlank()) {
                // Translators: button that checks a repository address.
                Text(tr("Check"))
            }
            state.remoteCheck?.takeIf { it.url == url.trim() }?.let { CheckResult(it.state, kind, onSwitchKind) }
        }
    }

    if (kind == Kind.DOWNLOAD) {
        PageTitle(tr("Where is your vault?"))
        Choice(selected = place == Place.DRIVE, onClick = { onPlace(Place.DRIVE) }) {
            ChoiceText(tr("Google Drive"), tr("A vault that Passel saved to your Google Drive."))
        }
        if (place == Place.DRIVE) DriveFields(kind, driveFileId, onDriveFile, state, model, onSwitchKind)
        Choice(selected = place == Place.GIT, onClick = { onPlace(Place.GIT) }) {
            ChoiceText(
                tr("Git repository"),
                tr(
                    "The HTTPS address of its repository, and a personal access token allowed to read and write it (GitHub: a fine-grained token with Contents read and write).",
                ),
                tag = tr("Advanced · recommended"),
            )
        }
        if (place == Place.GIT) remoteFields()
        Choice(selected = place == Place.PHONE, onClick = { onPlace(Place.PHONE) }) {
            ChoiceText(
                tr("On this phone"),
                tr("A folder or a file holding the vault: on this phone, an SD card, a USB drive, or in a storage app."),
            )
        }
    } else {
        PageTitle(tr("Where to keep your vault?"))
        Choice(selected = place == Place.DRIVE, onClick = { onPlace(Place.DRIVE) }) {
            ChoiceText(
                tr("Google Drive"),
                tr("Saved on your Google account and synced with your other phones: just sign in. Passel only sees the files it makes there."),
            )
        }
        if (place == Place.DRIVE) DriveFields(kind, driveFileId, onDriveFile, state, model, onSwitchKind)
        Choice(selected = place == Place.GIT, onClick = { onPlace(Place.GIT) }) {
            ChoiceText(
                tr("Git repository"),
                tr(
                    "An empty private repository you created for it (GitHub, GitLab…). Your vault is backed up with its whole history, and opens with pass and Passel on your computers too.",
                ),
                // Translators: tag of the git choice: it needs some technical knowledge, but is the most capable.
                tag = tr("Advanced · recommended"),
            )
        }
        if (place == Place.GIT) remoteFields()
        Choice(selected = place == Place.PHONE, onClick = { onPlace(Place.PHONE) }) {
            ChoiceText(
                tr("On this phone"),
                tr("With a copy in a folder or a file you choose: an SD card, a USB drive, or a folder synced by Syncthing or Nextcloud."),
            )
        }
    }
}

/** For a vault on this phone: its copy in a folder or a file the user picks. */
@Composable
private fun LocalPage(
    kind: Kind,
    local: Local,
    onLocal: (Local) -> Unit,
    pickedName: String?,
    onPicked: (String, String?) -> Unit,
) {
    val context = LocalContext.current
    fun picked(uri: Uri?) {
        if (uri == null) return
        LocalCopy.keepAccess(context, uri)
        onPicked(uri.toString(), LocalCopy.displayName(context, uri))
    }
    val pickFolder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree(), ::picked)
    val newFile = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream"), ::picked)
    val openFile = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument(), ::picked)

    @Composable
    fun Picker(onClick: () -> Unit) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = onClick) { Text(tr("Choose…")) }
            Text(pickedName ?: tr("Nothing chosen yet"), style = MaterialTheme.typography.bodyMedium)
        }
    }

    PageTitle(if (kind == Kind.CREATE) tr("Where on this phone?") else tr("Where is the vault on this phone?"))
    Choice(selected = local == Local.FOLDER, onClick = { onLocal(Local.FOLDER) }) {
        ChoiceText(
            tr("A folder"),
            if (kind == Kind.CREATE) {
                tr("Passel keeps a copy named “passwords.git” in the folder you choose: a git repository, which pass and Passel on a computer open too.")
            } else {
                tr("The “passwords.git” folder made by Passel or git, or the folder holding it.")
            },
            // Translators: tag of the recommended choice.
            tag = tr("Recommended"),
        )
    }
    if (local == Local.FOLDER) Picker { pickFolder.launch(null) }
    Choice(selected = local == Local.FILE, onClick = { onLocal(Local.FILE) }) {
        ChoiceText(
            tr("A file"),
            if (kind == Kind.CREATE) {
                tr("Passel keeps a copy in one “.bundle” file, easy to move or send. On a computer, git clone opens it.")
            } else {
                tr("A “.bundle” file made by Passel or git.")
            },
        )
    }
    if (local == Local.FILE) {
        Picker { if (kind == Kind.CREATE) newFile.launch(LocalCopy.FILE_NAME) else openFile.launch(arrayOf("*/*")) }
    }
}

/**
 * The Google Drive connection (a sign-in button only when Google needs
 * one), then the vault to download (or a hint when creating).
 */
@Composable
private fun DriveFields(
    kind: Kind,
    driveFileId: String?,
    onDriveFile: (String) -> Unit,
    state: VaultState,
    model: VaultViewModel,
    onSwitchKind: (Kind) -> Unit,
) {
    val vaults = state.driveVaults
    if (vaults == null) {
        if (state.driveSignIn) {
            Button(onClick = model::connectDrive, enabled = state.busy == null, modifier = Modifier.fillMaxWidth()) {
                Text(tr("Sign in with Google"))
            }
        } else {
            LaunchedEffect(Unit) { model.checkDrive() }
            Text(tr("Connecting to Google Drive"), style = MaterialTheme.typography.bodyMedium)
            LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
        }
        return
    }
    Text(tr("Connected to Google Drive"), color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.bodyMedium)
    when {
        kind == Kind.CREATE && vaults.isNotEmpty() -> Column {
            Text(
                trn(
                    "This Google Drive already holds {n} vault.",
                    "This Google Drive already holds {n} vaults.",
                    vaults.size,
                    "n" to vaults.size,
                ),
                style = MaterialTheme.typography.bodyMedium,
            )
            TextButton(onClick = { onSwitchKind(Kind.DOWNLOAD) }) { Text(tr("Download it instead")) }
        }
        kind == Kind.DOWNLOAD && vaults.isEmpty() -> Column {
            Text(tr("No Passel vault on this Google Drive"), color = MaterialTheme.colorScheme.error)
            TextButton(onClick = { onSwitchKind(Kind.CREATE) }) { Text(tr("Create a new vault there")) }
        }
        kind == Kind.DOWNLOAD -> vaults.forEach { file ->
            Choice(selected = file.id == driveFileId, onClick = { onDriveFile(file.id) }) {
                ChoiceText(file.vaultName, file.modifiedTime?.let(::modifiedLabel).orEmpty())
            }
        }
    }
}

/** `2026-11-27T10:58:18.000Z` → "Changed on 27 Nov 2026" (in the device's style). */
private fun modifiedLabel(time: String): String = runCatching {
    val instant = java.time.Instant.parse(time)
    val date = java.time.format.DateTimeFormatter.ofLocalizedDate(java.time.format.FormatStyle.MEDIUM)
        .format(instant.atZone(java.time.ZoneId.systemDefault()))
    tr("Changed on {date}", "date" to date)
}.getOrDefault("")

/** What "Check" found, with a way out when it is the other kind of vault. */
@Composable
private fun CheckResult(found: RemoteState?, kind: Kind, onSwitchKind: (Kind) -> Unit) {
    val good = MaterialTheme.colorScheme.primary
    val bad = MaterialTheme.colorScheme.error
    val (text, color) = when (found) {
        null -> tr("Checking…") to Color.Unspecified
        RemoteState.NOT_EMPTY ->
            if (kind == Kind.DOWNLOAD) tr("Found a vault") to good else tr("It already holds a vault") to bad
        RemoteState.EMPTY ->
            if (kind == Kind.CREATE) tr("Empty: ready for a new vault") to good else tr("This repository is empty") to bad
        RemoteState.ACCESS_DENIED -> tr("The server refused this username or token") to bad
        RemoteState.UNKNOWN_HOST -> tr("Unknown server key") to bad
        RemoteState.UNREACHABLE -> tr("Nothing found at this address") to bad
        RemoteState.NOT_HTTPS -> tr("Use an https:// address") to bad
    }
    Column {
        Text(text, color = color, style = MaterialTheme.typography.bodyMedium)
        when {
            found == RemoteState.EMPTY && kind == Kind.DOWNLOAD ->
                TextButton(onClick = { onSwitchKind(Kind.CREATE) }) { Text(tr("Create a new vault there")) }
            found == RemoteState.NOT_EMPTY && kind == Kind.CREATE ->
                TextButton(onClick = { onSwitchKind(Kind.DOWNLOAD) }) { Text(tr("Download it instead")) }
        }
    }
}

@Composable
private fun IdentityPage(
    vaultName: String,
    onVaultName: (String) -> Unit,
    author: String,
    onAuthor: (String) -> Unit,
    email: String,
    onEmail: (String) -> Unit,
) {
    PageTitle(tr("Last step"))
    OutlinedTextField(
        vaultName,
        onVaultName,
        label = { Text(tr("Name of the vault on this phone")) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    Text(tr("The changes you make from this phone are signed with:"), style = MaterialTheme.typography.bodyMedium)
    OutlinedTextField(author, onAuthor, label = { Text(tr("Your name")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(
        email,
        onEmail,
        label = { Text(tr("Your email")) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
        modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun PageTitle(text: String) {
    Text(text, style = MaterialTheme.typography.headlineSmall)
}

/** A selectable card with a radio button. */
@Composable
private fun Choice(selected: Boolean, onClick: () -> Unit, content: @Composable () -> Unit) {
    OutlinedCard(
        Modifier
            .fillMaxWidth()
            .selectable(selected = selected, onClick = onClick, role = Role.RadioButton),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            RadioButton(selected = selected, onClick = null)
            Column(Modifier.padding(start = 8.dp).weight(1f)) { content() }
        }
    }
}

/**
 * After downloading a vault whose key is not on this phone: import the
 * backup of that key, or do it later (the vault screen offers it again).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun VaultKeyPage(need: VaultKeyNeed, state: VaultState, model: VaultViewModel) {
    val pickKey = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(model::importKey)
    }
    val idle = state.busy == null
    BackHandler { model.finishVaultKey() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(tr("Add a vault")) },
                navigationIcon = {
                    IconButton(onClick = model::finishVaultKey, enabled = idle) {
                        Icon(Icons.Default.Close, contentDescription = tr("Later"))
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            PageTitle(tr("Your encryption key"))
            Text(
                tr(
                    "{name} is downloaded. Its entries are encrypted for this key, which is not on this phone yet: import its backup to open them.",
                    "name" to need.vaultName,
                ),
            )
            need.recipients.forEach { Text(keyLabel(it), style = MaterialTheme.typography.bodyMedium) }
            state.busy?.let {
                Text(it)
                LinearProgressIndicator(Modifier.fillMaxWidth())
            }
            Row(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                TextButton(onClick = model::finishVaultKey, enabled = idle) { Text(tr("Later")) }
                Button(onClick = { pickKey.launch(arrayOf("*/*")) }, enabled = idle) { Text(tr("Import a key")) }
            }
        }
    }
}

/** A `.gpg-id` line as shown: a fingerprint in groups of four. */
fun keyLabel(recipient: String): String =
    if (recipient.length >= 16 && recipient.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) {
        recipient.uppercase().chunked(4).joinToString(" ")
    } else {
        recipient
    }

/** A choice's title (with an optional [tag] beside it) and explanation. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ChoiceText(title: String, detail: String, tag: String? = null) {
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        itemVerticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium)
        if (tag != null) {
            Surface(
                shape = MaterialTheme.shapes.small,
                color = MaterialTheme.colorScheme.secondaryContainer,
                contentColor = MaterialTheme.colorScheme.onSecondaryContainer,
            ) {
                Text(tag, style = MaterialTheme.typography.labelMedium, modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp))
            }
        }
    }
    if (detail.isNotEmpty()) Text(detail, style = MaterialTheme.typography.bodyMedium)
}

@Composable
fun KeyLine(key: KeyInfo, model: VaultViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    Column {
        // Translators: shown for a key without a name.
        Text(key.userId ?: tr("Key"), style = MaterialTheme.typography.bodyLarge)
        Text(key.fingerprint.chunked(4).joinToString(" "), style = MaterialTheme.typography.bodySmall)
        val saveBackup = rememberKeyBackupLauncher(model)
        Row {
            TextButton(onClick = {
                scope.launch {
                    val armored = model.publicKey(key.fingerprint) ?: return@launch
                    val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, armored)
                    context.startActivity(Intent.createChooser(send, tr("Share the public key")))
                }
            }) { Text(tr("Share public key")) }
            TextButton(onClick = { saveBackup(key.fingerprint) }) { Text(tr("Save a backup")) }
        }
    }
}

/** `Name <email>` → (Name, email); a bare name or email fills one side. */
private fun splitUserId(userId: String?): Pair<String, String> {
    val id = userId?.trim().orEmpty()
    val open = id.lastIndexOf('<')
    val close = id.lastIndexOf('>')
    return when {
        open in 0 until close -> id.substring(0, open).trim() to id.substring(open + 1, close).trim()
        '@' in id -> "" to id
        else -> id to ""
    }
}

/** `https://host/me/work-vault.git` → `Work-vault`. */
private fun repositoryName(url: String): String? =
    url.trim().trimEnd('/').substringAfterLast('/').removeSuffix(".git")
        .takeIf { it.isNotBlank() && "://" !in it }
        ?.replaceFirstChar { it.uppercase() }
