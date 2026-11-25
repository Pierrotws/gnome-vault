package io.pierrotws.passel

import android.content.Intent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.pierrotws.passel.core.KeyInfo
import kotlinx.coroutines.launch

/** First run: get a key, then clone the vault. */
@Composable
fun SetupScreen(state: VaultState, model: VaultViewModel) {
    Column(
        Modifier
            .fillMaxSize()
            .safeDrawingPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Welcome to Passel", style = MaterialTheme.typography.headlineMedium)
        Text("Your passwords stay in your own pass vault, encrypted with your own key.")
        state.busy?.let {
            Text(it)
            LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        KeyCard(state.keys, state.busy == null, model)
        VaultCard(enabled = state.keys.isNotEmpty() && state.busy == null, model)
    }
}

@Composable
private fun KeyCard(keys: List<KeyInfo>, idle: Boolean, model: VaultViewModel) {
    val pickKey = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let(model::importKey)
    }
    var creating by rememberSaveable { mutableStateOf(false) }
    var name by rememberSaveable { mutableStateOf("") }
    var email by rememberSaveable { mutableStateOf("") }

    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("1. Your key", style = MaterialTheme.typography.titleMedium)
            if (keys.isNotEmpty()) {
                keys.forEach { KeyLine(it, model) }
                return@Column
            }
            Text(
                "Import the backup of the key your vault is encrypted with " +
                    "(on a computer: gpg --export-secret-keys --armor <id>), or create a new one.",
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { pickKey.launch(arrayOf("*/*")) }, enabled = idle) { Text("Import a key") }
                OutlinedButton(onClick = { creating = !creating }, enabled = idle) { Text("Create a key") }
            }
            if (creating) {
                OutlinedTextField(name, { name = it }, label = { Text("Your name") }, singleLine = true)
                OutlinedTextField(
                    email,
                    { email = it },
                    label = { Text("Email (optional)") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                )
                Button(onClick = { model.createKey(name, email) }, enabled = idle && name.isNotBlank()) {
                    Text("Create")
                }
            }
        }
    }
}

@Composable
fun KeyLine(key: KeyInfo, model: VaultViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    Column {
        Text(key.userId ?: "Key", style = MaterialTheme.typography.bodyLarge)
        Text(key.fingerprint.chunked(4).joinToString(" "), style = MaterialTheme.typography.bodySmall)
        OutlinedButton(onClick = {
            scope.launch {
                val armored = model.publicKey(key.fingerprint) ?: return@launch
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, armored)
                context.startActivity(Intent.createChooser(send, "Share the public key"))
            }
        }) { Text("Share public key") }
    }
}

@Composable
private fun VaultCard(enabled: Boolean, model: VaultViewModel) {
    var url by rememberSaveable { mutableStateOf("") }
    var username by rememberSaveable { mutableStateOf("") }
    var token by rememberSaveable { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    var email by rememberSaveable { mutableStateOf("") }

    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("2. Your vault", style = MaterialTheme.typography.titleMedium)
            Text(
                "The HTTPS address of your vault's repository, and a personal access token " +
                    "allowed to read and write it.",
            )
            OutlinedTextField(
                url,
                { url = it },
                label = { Text("Address (https://…)") },
                singleLine = true,
                enabled = enabled,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                username,
                { username = it },
                label = { Text("Username") },
                singleLine = true,
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            )
            PasswordField(token, { token = it }, "Access token", Modifier.fillMaxWidth(), enabled = enabled)
            Text("Your changes from this phone are signed:", style = MaterialTheme.typography.bodySmall)
            OutlinedTextField(
                name,
                { name = it },
                label = { Text("Name") },
                singleLine = true,
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                email,
                { email = it },
                label = { Text("Email") },
                singleLine = true,
                enabled = enabled,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                modifier = Modifier.fillMaxWidth(),
            )
            Button(
                onClick = { model.cloneVault(url, username, token, name, email) },
                enabled = enabled && url.isNotBlank() && name.isNotBlank() && email.isNotBlank(),
            ) { Text("Download the vault") }
        }
    }
}
