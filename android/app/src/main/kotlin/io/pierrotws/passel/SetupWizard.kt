package io.pierrotws.passel

import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
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

private enum class Step { WELCOME, KEY, KIND, PLACE, IDENTITY }

private enum class Kind { DOWNLOAD, CREATE }

/**
 * Sets up a vault, step by step: on first run (from a welcome page) and
 * from "Add a vault". Like the desktop wizard: the key, then an existing
 * vault to download or a new one to create, where it lives, and who signs
 * this device's changes.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SetupWizard(state: VaultState, model: VaultViewModel) {
    val firstRun = state.vaults.isEmpty()
    var step by rememberSaveable { mutableStateOf(if (firstRun) Step.WELCOME else Step.KEY) }
    var kind by rememberSaveable { mutableStateOf<Kind?>(null) }
    var fingerprint by rememberSaveable { mutableStateOf<String?>(null) }
    var online by rememberSaveable { mutableStateOf(true) }
    var url by rememberSaveable { mutableStateOf("") }
    var username by rememberSaveable { mutableStateOf("") }
    // Not saved in the instance state: it is a secret.
    var token by remember { mutableStateOf("") }
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
        val key = state.keys.firstOrNull { it.fingerprint == fingerprint }
        val (keyName, keyEmail) = splitUserId(key?.userId)
        if (author.isBlank()) author = keyName
        if (email.isBlank()) email = keyEmail
        if (vaultName.isBlank()) {
            vaultName = repositoryName(url).takeIf { kind == Kind.DOWNLOAD || online }
                ?: if (firstRun) "Passwords" else "Vault"
        }
    }

    val order = listOfNotNull(Step.WELCOME.takeIf { firstRun }, Step.KEY, Step.KIND, Step.PLACE, Step.IDENTITY)
    val index = order.indexOf(step)
    val idle = state.busy == null
    fun back() {
        if (index > 0) step = order[index - 1] else model.stopAddingVault()
    }
    BackHandler(enabled = index > 0 || !firstRun) { back() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (firstRun) "Set up Passel" else "Add a vault") },
                navigationIcon = {
                    when {
                        index > 0 -> IconButton(onClick = ::back, enabled = idle) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                        !firstRun -> IconButton(onClick = model::stopAddingVault) {
                            Icon(Icons.Default.Close, contentDescription = "Cancel")
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
            Text("Step ${index + 1} of ${order.size}", style = MaterialTheme.typography.labelMedium)
            when (step) {
                Step.WELCOME -> WelcomePage(onStart = { step = Step.KEY })
                Step.KEY -> KeyPage(state, fingerprint, onSelect = { fingerprint = it }, model)
                Step.KIND -> KindPage(kind) { kind = it; step = Step.PLACE }
                Step.PLACE -> PlacePage(
                    kind = kind ?: Kind.DOWNLOAD,
                    online = online,
                    onOnline = { online = it },
                    url = url,
                    onUrl = { url = it },
                    username = username,
                    onUsername = { username = it },
                    token = token,
                    onToken = { token = it },
                    state = state,
                    model = model,
                    onSwitchKind = { kind = it },
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
                    Step.KEY -> Button(onClick = { step = Step.KIND }, enabled = idle && fingerprint != null) { Text("Next") }
                    Step.PLACE -> Button(
                        onClick = { step = Step.IDENTITY },
                        enabled = idle && (url.isNotBlank() || (kind == Kind.CREATE && !online)),
                    ) { Text("Next") }
                    Step.IDENTITY -> Button(
                        onClick = {
                            when (kind) {
                                Kind.CREATE -> model.createVault(
                                    vaultName, fingerprint!!, url.takeIf { online }, username, token, author, email,
                                )
                                else -> model.cloneVault(vaultName, url, username, token, author, email)
                            }
                        },
                        enabled = idle && vaultName.isNotBlank() && author.isNotBlank() && email.isNotBlank(),
                    ) { Text(if (kind == Kind.CREATE) "Create the vault" else "Download the vault") }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

@Composable
private fun WelcomePage(onStart: () -> Unit) {
    Text("Welcome to Passel", style = MaterialTheme.typography.headlineMedium)
    Text(
        "Passel keeps your passwords in a pass vault: one encrypted file per entry, " +
            "with its whole history, in a git repository you choose. The same vault opens " +
            "on your computers and phones.",
    )
    Text("Setting it up takes a few steps: your key, then your vault.")
    Button(onClick = onStart, modifier = Modifier.fillMaxWidth()) { Text("Get started") }
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

    PageTitle("Your encryption key")
    Text(
        "Entries are encrypted with an OpenPGP key. To open a vault made on a computer, " +
            "import the backup of its key (gpg --export-secret-keys --armor <id>). " +
            "A new vault can use a key created here.",
    )
    state.keys.forEach { key ->
        Choice(selected = key.fingerprint == selected, onClick = { onSelect(key.fingerprint) }) {
            KeyLine(key, model)
        }
    }
    Text(if (state.keys.isEmpty()) "Get a key:" else "Or add another key:", style = MaterialTheme.typography.titleSmall)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = { pickKey.launch(arrayOf("*/*")) }, enabled = idle) { Text("Import a key") }
        OutlinedButton(onClick = { creating = !creating }, enabled = idle) { Text("Create a key") }
    }
    if (creating) {
        OutlinedTextField(name, { name = it }, label = { Text("Your name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(
            email,
            { email = it },
            label = { Text("Email (optional)") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
            modifier = Modifier.fillMaxWidth(),
        )
        Button(onClick = { model.createKey(name, email); creating = false }, enabled = idle && name.isNotBlank()) {
            Text("Create")
        }
    }
}

@Composable
private fun KindPage(selected: Kind?, onSelect: (Kind) -> Unit) {
    PageTitle("Your vault")
    Choice(selected = selected == Kind.DOWNLOAD, onClick = { onSelect(Kind.DOWNLOAD) }) {
        ChoiceText("I already have a vault", "Download it from its repository: made with Passel, or with pass and git.")
    }
    Choice(selected = selected == Kind.CREATE, onClick = { onSelect(Kind.CREATE) }) {
        ChoiceText("Create a new vault", "Start an empty vault, encrypted with your key.")
    }
}

@Composable
private fun PlacePage(
    kind: Kind,
    online: Boolean,
    onOnline: (Boolean) -> Unit,
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
            label = { Text("Address (https://…)") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(username, onUsername, label = { Text("Username") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        PasswordField(token, onToken, "Access token", Modifier.fillMaxWidth())
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = { model.checkRemote(url, username, token) }, enabled = url.isNotBlank()) {
                Text("Check")
            }
            state.remoteCheck?.takeIf { it.url == url.trim() }?.let { CheckResult(it.state, kind, onSwitchKind) }
        }
    }

    if (kind == Kind.DOWNLOAD) {
        PageTitle("Where is your vault?")
        Text(
            "The HTTPS address of its repository, and a personal access token allowed " +
                "to read and write it (GitHub: a fine-grained token with Contents read and write).",
        )
        remoteFields()
    } else {
        PageTitle("Where to keep a copy?")
        Choice(selected = online, onClick = { onOnline(true) }) {
            ChoiceText(
                "Online, in an empty repository",
                "Recommended: a private repository you created for it (GitHub, GitLab…). " +
                    "Your vault is backed up, and your other devices sync with it.",
            )
        }
        if (online) remoteFields()
        Choice(selected = !online, onClick = { onOnline(false) }) {
            ChoiceText("Only on this phone", "No backup: losing this phone loses this vault.")
        }
    }
}

/** What "Check" found, with a way out when it is the other kind of vault. */
@Composable
private fun CheckResult(found: RemoteState?, kind: Kind, onSwitchKind: (Kind) -> Unit) {
    val good = MaterialTheme.colorScheme.primary
    val bad = MaterialTheme.colorScheme.error
    val (text, color) = when (found) {
        null -> "Checking…" to Color.Unspecified
        RemoteState.NOT_EMPTY ->
            if (kind == Kind.DOWNLOAD) "Found a vault" to good else "It already holds a vault" to bad
        RemoteState.EMPTY ->
            if (kind == Kind.CREATE) "Empty: ready for a new vault" to good else "This repository is empty" to bad
        RemoteState.ACCESS_DENIED -> "The server refused this username or token" to bad
        RemoteState.UNKNOWN_HOST -> "Unknown server key" to bad
        RemoteState.UNREACHABLE -> "Nothing found at this address" to bad
    }
    Column {
        Text(text, color = color, style = MaterialTheme.typography.bodyMedium)
        when {
            found == RemoteState.EMPTY && kind == Kind.DOWNLOAD ->
                TextButton(onClick = { onSwitchKind(Kind.CREATE) }) { Text("Create a new vault there") }
            found == RemoteState.NOT_EMPTY && kind == Kind.CREATE ->
                TextButton(onClick = { onSwitchKind(Kind.DOWNLOAD) }) { Text("Download it instead") }
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
    PageTitle("Last step")
    OutlinedTextField(
        vaultName,
        onVaultName,
        label = { Text("Name of the vault on this phone") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    Text("The changes you make from this phone are signed with:", style = MaterialTheme.typography.bodyMedium)
    OutlinedTextField(author, onAuthor, label = { Text("Your name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
    OutlinedTextField(
        email,
        onEmail,
        label = { Text("Your email") },
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

@Composable
private fun ChoiceText(title: String, detail: String) {
    Text(title, style = MaterialTheme.typography.titleMedium)
    Text(detail, style = MaterialTheme.typography.bodyMedium)
}

@Composable
fun KeyLine(key: KeyInfo, model: VaultViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    Column {
        Text(key.userId ?: "Key", style = MaterialTheme.typography.bodyLarge)
        Text(key.fingerprint.chunked(4).joinToString(" "), style = MaterialTheme.typography.bodySmall)
        TextButton(onClick = {
            scope.launch {
                val armored = model.publicKey(key.fingerprint) ?: return@launch
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, armored)
                context.startActivity(Intent.createChooser(send, "Share the public key"))
            }
        }) { Text("Share public key") }
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
