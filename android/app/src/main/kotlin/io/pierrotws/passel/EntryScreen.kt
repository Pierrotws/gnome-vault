package io.pierrotws.passel

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
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
import androidx.compose.material3.CircularProgressIndicator
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
import androidx.compose.runtime.produceState
import androidx.compose.runtime.withFrameMillis
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.pierrotws.passel.core.Entry
import io.pierrotws.passel.core.Field
import io.pierrotws.passel.core.FieldKind
import io.pierrotws.passel.core.OtpCode
import io.pierrotws.passel.core.PasselException
import io.pierrotws.passel.core.generatePassword
import io.pierrotws.passel.core.otpCode

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
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = tr("Back"))
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
                            Icon(Icons.Default.Close, contentDescription = tr("Cancel"))
                        }
                        IconButton(
                            // Stay in edit mode until it is saved: a failed save
                            // keeps the edits on screen.
                            onClick = { model.save(open.ref, draft) { editing = false } },
                            enabled = busy == null && draft != open.entry,
                        ) { Icon(Icons.Default.Check, contentDescription = tr("Save")) }
                    } else {
                        IconButton(onClick = { editing = true }, enabled = busy == null) {
                            Icon(Icons.Default.Edit, contentDescription = tr("Edit"))
                        }
                        IconButton(onClick = { confirmDelete = true }, enabled = busy == null) {
                            Icon(Icons.Default.Delete, contentDescription = tr("Delete"))
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
                SecretRow(tr("Password"), open.entry.password, model)
                open.entry.fields.forEach { FieldRow(it, model) }
            }
        }
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text(tr("Delete {name}?", "name" to open.ref.name)) },
            text = { Text(tr("It stays in the vault's history.")) },
            confirmButton = {
                TextButton(onClick = { confirmDelete = false; model.delete(open.ref) }) { Text(tr("Delete")) }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text(tr("Cancel")) } },
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
                    IconButton(onClick = model::close) { Icon(Icons.Default.Close, contentDescription = tr("Cancel")) }
                },
                title = { Text(tr("New entry")) },
                actions = {
                    IconButton(
                        onClick = { model.create(folder, name, draft) },
                        enabled = busy == null && name.isNotBlank(),
                    ) { Icon(Icons.Default.Check, contentDescription = tr("Save")) }
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
            OutlinedTextField(folder, { folder = it }, label = { Text(tr("Folder (e.g. web)")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(name, { name = it }, label = { Text(tr("Name (e.g. github)")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
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
            label = { Text(tr("Password")) },
            singleLine = true,
            textStyle = MaterialTheme.typography.bodyLarge.copy(fontFamily = FontFamily.Monospace),
            modifier = Modifier.weight(1f),
        )
        OutlinedButton(onClick = {
            runCatching { generatePassword(24u, true) }.onSuccess { onChange(entry.copy(password = it)) }
        }) { Text(tr("Generate")) }
    }
    entry.fields.forEachIndexed { index, field ->
        Card(Modifier.fillMaxWidth()) {
            Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        field.name,
                        { name -> onChange(entry.withField(index, field.copy(name = name))) },
                        // Translators: the name of an entry's field, like "login" or "url".
                        label = { Text(tr("Field")) },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    IconButton(onClick = { onChange(entry.copy(fields = entry.fields.filterIndexed { i, _ -> i != index })) }) {
                        Icon(Icons.Default.Delete, contentDescription = tr("Remove field"))
                    }
                }
                OutlinedTextField(
                    field.value,
                    { value -> onChange(entry.withField(index, field.copy(value = value))) },
                    // Translators: the value of an entry's field.
                    label = { Text(tr("Value")) },
                    singleLine = field.kind != FieldKind.MULTILINE,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
    OutlinedButton(onClick = { onChange(entry.copy(fields = entry.fields + Field("", "", FieldKind.TEXT))) }) {
        Icon(Icons.Default.Add, contentDescription = null)
        Text(tr("Add a field"))
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
            TextButton(onClick = { model.copy(field.value, field.name) }) { Text(tr("Copy")) }
        }
    }
}

@Composable
private fun SecretRow(label: String, value: String, model: VaultViewModel) {
    var shown by remember(value) { mutableStateOf(false) }
    LabeledValue(label, if (shown) value else "•".repeat(minOf(value.length, 12)), monospace = true) {
        TextButton(onClick = { shown = !shown }) { Text(if (shown) tr("Hide") else tr("Show")) }
        TextButton(onClick = { model.copy(value, label) }) { Text(tr("Copy")) }
    }
}

/**
 * The current code, with a ring that empties smoothly until the next one
 * (red for the last 5 seconds); each new code slides in.
 */
@Composable
private fun OtpRow(label: String, uri: String, model: VaultViewModel) {
    var otp by remember(uri) { mutableStateOf<OtpCode?>(null) }
    var error by remember(uri) { mutableStateOf<String?>(null) }
    // Wall-clock time, refreshed every frame while the row is shown.
    val now by produceState(System.currentTimeMillis(), uri) {
        while (true) withFrameMillis { value = System.currentTimeMillis() }
    }
    val periodMs = (otp?.period?.toLong() ?: 30L).coerceAtLeast(1L) * 1000L
    val window = now / periodMs
    // A new code is due at each period boundary.
    LaunchedEffect(uri, window) {
        try {
            otp = otpCode(uri)
        } catch (e: PasselException) {
            error = (e as? PasselException.Failed)?.reason ?: tr("Invalid one-time password")
        }
    }
    val leftMs = periodMs - now % periodMs
    val urgent = leftMs <= 5_000
    val accent by animateColorAsState(
        if (urgent) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
        label = "otp color",
    )

    Card(Modifier.fillMaxWidth()) {
        Row(Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
                val failure = error
                if (failure != null) {
                    Text(failure, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.error)
                } else {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 4.dp)) {
                        Box(contentAlignment = Alignment.Center, modifier = Modifier.size(36.dp)) {
                            CircularProgressIndicator(
                                progress = { leftMs.toFloat() / periodMs },
                                modifier = Modifier.fillMaxSize(),
                                color = accent,
                                strokeWidth = 3.dp,
                                trackColor = MaterialTheme.colorScheme.outlineVariant,
                            )
                            Text("${(leftMs + 999) / 1000}", style = MaterialTheme.typography.labelMedium, color = accent)
                        }
                        Spacer(Modifier.width(12.dp))
                        AnimatedContent(
                            targetState = otp?.code.orEmpty(),
                            transitionSpec = {
                                (slideInVertically { it } + fadeIn()) togetherWith (slideOutVertically { -it } + fadeOut())
                            },
                            label = "otp code",
                        ) { code ->
                            Text(
                                code.chunked(3).joinToString(" "),
                                style = MaterialTheme.typography.headlineSmall.copy(fontFamily = FontFamily.Monospace),
                                color = if (urgent) accent else MaterialTheme.colorScheme.onSurface,
                            )
                        }
                    }
                }
            }
            otp?.let { current ->
                // Translators: a one-time password code, as in "Code copied".
                TextButton(onClick = { model.copy(current.code, tr("Code")) }) { Text(tr("Copy")) }
            }
        }
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
