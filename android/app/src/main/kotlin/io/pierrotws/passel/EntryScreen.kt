package io.pierrotws.passel

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.pierrotws.passel.core.Entry
import io.pierrotws.passel.core.Field
import io.pierrotws.passel.core.FieldKind
import io.pierrotws.passel.core.PasselException
import io.pierrotws.passel.core.generatePassword
import io.pierrotws.passel.core.otpCode
import kotlinx.coroutines.delay

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EntryScreen(open: OpenEntry, busy: String?, model: VaultViewModel) {
    var editing by remember(open.ref.path) { mutableStateOf(false) }
    var draft by remember(open) { mutableStateOf(open.entry) }
    var confirmDelete by remember { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = {
                    IconButton(onClick = model::close) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                title = {
                    Column {
                        Text(open.ref.name)
                        if (open.ref.folder.isNotEmpty()) {
                            Text(open.ref.folder, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                },
                actions = {
                    if (editing) {
                        IconButton(onClick = { draft = open.entry; editing = false }) {
                            Icon(Icons.Default.Close, contentDescription = "Cancel")
                        }
                        IconButton(
                            // Stay in edit mode until it is saved: a failed save
                            // keeps the edits on screen.
                            onClick = { model.save(open.ref, draft) { editing = false } },
                            enabled = busy == null && draft != open.entry,
                        ) { Icon(Icons.Default.Check, contentDescription = "Save") }
                    } else {
                        IconButton(onClick = { editing = true }, enabled = busy == null) {
                            Icon(Icons.Default.Edit, contentDescription = "Edit")
                        }
                        IconButton(onClick = { confirmDelete = true }, enabled = busy == null) {
                            Icon(Icons.Default.Delete, contentDescription = "Delete")
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
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (busy != null) LinearProgressIndicator(Modifier.fillMaxWidth())
            if (editing) {
                EntryEditor(draft, onChange = { draft = it })
            } else {
                SecretRow("Password", open.entry.password, model)
                open.entry.fields.forEach { FieldRow(it, model) }
            }
        }
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete ${open.ref.name}?") },
            text = { Text("It stays in the vault's history.") },
            confirmButton = {
                TextButton(onClick = { confirmDelete = false; model.delete(open.ref) }) { Text("Delete") }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewEntryScreen(busy: String?, model: VaultViewModel) {
    var folder by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var draft by remember {
        mutableStateOf(
            Entry(
                password = runCatching { generatePassword(24u, true) }.getOrDefault(""),
                fields = listOf(Field("login", "", FieldKind.TEXT)),
            ),
        )
    }
    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = {
                    IconButton(onClick = model::close) { Icon(Icons.Default.Close, contentDescription = "Cancel") }
                },
                title = { Text("New entry") },
                actions = {
                    IconButton(
                        onClick = { model.create(folder, name, draft) },
                        enabled = busy == null && name.isNotBlank(),
                    ) { Icon(Icons.Default.Check, contentDescription = "Save") }
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
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (busy != null) LinearProgressIndicator(Modifier.fillMaxWidth())
            OutlinedTextField(folder, { folder = it }, label = { Text("Folder (e.g. web)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(name, { name = it }, label = { Text("Name (e.g. github)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            EntryEditor(draft, onChange = { draft = it })
        }
    }
}

/** Edits the password and the fields of an entry. */
@Composable
private fun EntryEditor(entry: Entry, onChange: (Entry) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            entry.password,
            { onChange(entry.copy(password = it)) },
            label = { Text("Password") },
            singleLine = true,
            textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = FontFamily.Monospace),
            modifier = Modifier.weight(1f),
        )
        OutlinedButton(onClick = {
            runCatching { generatePassword(24u, true) }.onSuccess { onChange(entry.copy(password = it)) }
        }) { Text("Generate") }
    }
    entry.fields.forEachIndexed { index, field ->
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        field.name,
                        { name -> onChange(entry.withField(index, field.copy(name = name))) },
                        label = { Text("Field") },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = { onChange(entry.copy(fields = entry.fields.filterIndexed { i, _ -> i != index })) }) {
                        Icon(Icons.Default.Delete, contentDescription = "Remove field")
                    }
                }
                OutlinedTextField(
                    field.value,
                    { value -> onChange(entry.withField(index, field.copy(value = value))) },
                    label = { Text("Value") },
                    singleLine = field.kind != FieldKind.MULTILINE,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
    OutlinedButton(onClick = { onChange(entry.copy(fields = entry.fields + Field("", "", FieldKind.TEXT))) }) {
        Icon(Icons.Default.Add, contentDescription = null)
        Text("Add a field")
    }
}

private fun Entry.withField(index: Int, field: Field) =
    copy(fields = fields.mapIndexed { i, old -> if (i == index) field else old })

@Composable
private fun FieldRow(field: Field, model: VaultViewModel) {
    when (field.kind) {
        FieldKind.PASSWORD -> SecretRow(field.name, field.value, model)
        FieldKind.OTP -> OtpRow(field.name, field.value, model)
        FieldKind.TEXT, FieldKind.MULTILINE -> LabeledValue(field.name, field.value) {
            TextButton(onClick = { model.copy(field.value, field.name) }) { Text("Copy") }
        }
    }
}

@Composable
private fun SecretRow(label: String, value: String, model: VaultViewModel) {
    var shown by remember(value) { mutableStateOf(false) }
    LabeledValue(label, if (shown) value else "•".repeat(minOf(value.length, 12)), monospace = true) {
        TextButton(onClick = { shown = !shown }) { Text(if (shown) "Hide" else "Show") }
        TextButton(onClick = { model.copy(value, label) }) { Text("Copy") }
    }
}

/** The current code, refreshed every second. */
@Composable
private fun OtpRow(label: String, uri: String, model: VaultViewModel) {
    var code by remember(uri) { mutableStateOf<String?>(null) }
    var secondsLeft by remember(uri) { mutableStateOf(0u) }
    var error by remember(uri) { mutableStateOf<String?>(null) }
    LaunchedEffect(uri) {
        while (true) {
            try {
                val otp = otpCode(uri)
                code = otp.code
                secondsLeft = otp.secondsLeft
            } catch (e: PasselException) {
                error = (e as? PasselException.Failed)?.reason ?: "Invalid one-time password"
                break
            }
            delay(1000)
        }
    }
    val shown = error ?: code?.let { it.chunked(3).joinToString(" ") + "  (${secondsLeft}s)" } ?: "…"
    LabeledValue(label, shown, monospace = error == null) {
        code?.let { current -> TextButton(onClick = { model.copy(current, "Code") }) { Text("Copy") } }
    }
}

@Composable
private fun LabeledValue(
    label: String,
    value: String,
    monospace: Boolean = false,
    actions: @Composable () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Row(Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                Text(
                    value,
                    style = MaterialTheme.typography.bodyLarge.let {
                        if (monospace) it.copy(fontFamily = FontFamily.Monospace) else it
                    },
                )
            }
            actions()
        }
    }
}
