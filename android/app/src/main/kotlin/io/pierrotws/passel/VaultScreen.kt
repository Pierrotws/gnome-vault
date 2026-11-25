package io.pierrotws.passel

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VaultScreen(state: VaultState, model: VaultViewModel) {
    var menu by remember { mutableStateOf(false) }
    var switcher by remember { mutableStateOf(false) }
    var confirmRemove by remember { mutableStateOf(false) }
    val active = state.activeVault

    Scaffold(
        topBar = {
            TopAppBar(
                // The open vault; tap to switch or add one.
                title = {
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.clickable(onClickLabel = "Switch vault") { switcher = true },
                    ) {
                        Text(active?.name ?: "Passel", maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Icon(Icons.Default.ArrowDropDown, contentDescription = null)
                    }
                    DropdownMenu(expanded = switcher, onDismissRequest = { switcher = false }) {
                        state.vaults.forEach { vault ->
                            DropdownMenuItem(
                                text = { Text(vault.name) },
                                leadingIcon = {
                                    if (vault.active) Icon(Icons.Default.Check, contentDescription = "Open")
                                },
                                onClick = {
                                    switcher = false
                                    if (!vault.active) model.switchVault(vault.id)
                                },
                            )
                        }
                        HorizontalDivider()
                        DropdownMenuItem(
                            text = { Text("Add a vault") },
                            leadingIcon = { Icon(Icons.Default.Add, contentDescription = null) },
                            onClick = { switcher = false; model.startAddingVault() },
                        )
                    }
                },
                actions = {
                    IconButton(onClick = { model.sync() }, enabled = state.busy == null) {
                        BadgedBox(badge = { if (state.unsynced > 0) Badge { Text("${state.unsynced}") } }) {
                            Icon(Icons.Default.Refresh, contentDescription = "Sync")
                        }
                    }
                    IconButton(onClick = model::lock) { Icon(Icons.Default.Lock, contentDescription = "Lock") }
                    IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, contentDescription = "More") }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(
                            text = { Text("Add a vault") },
                            onClick = { menu = false; model.startAddingVault() },
                        )
                        DropdownMenuItem(
                            text = { Text("Remove ${active?.name ?: "this vault"} from this phone") },
                            onClick = { menu = false; confirmRemove = true },
                        )
                    }
                },
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = model::startCreating) {
                Icon(Icons.Default.Add, contentDescription = "New entry")
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            if (state.busy != null) LinearProgressIndicator(Modifier.fillMaxWidth())
            OutlinedTextField(
                value = state.query,
                onValueChange = model::setQuery,
                placeholder = { Text("Search ${state.entries.size} entries") },
                leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
                singleLine = true,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )
            val groups = state.filtered.groupBy { it.folder }
            LazyColumn(Modifier.fillMaxSize()) {
                groups.forEach { (folder, entries) ->
                    if (folder.isNotEmpty()) {
                        item(key = "folder:$folder") {
                            Text(
                                folder,
                                style = MaterialTheme.typography.labelLarge,
                                color = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.padding(start = 16.dp, top = 16.dp, bottom = 4.dp),
                            )
                        }
                    }
                    items(entries, key = { it.path }) { entry ->
                        ListItem(
                            modifier = Modifier.clickable(enabled = state.busy == null) { model.open(entry) },
                        ) { Text(entry.name) }
                        HorizontalDivider()
                    }
                }
            }
        }
    }

    if (confirmRemove && active != null) {
        AlertDialog(
            onDismissRequest = { confirmRemove = false },
            title = { Text("Remove ${active.name} from this phone?") },
            text = {
                val unsynced = if (state.unsynced > 0) " ${state.unsynced} change(s) not synced yet will be lost." else ""
                Text("The copy on your remote stays as it is; you can download it again.$unsynced")
            },
            confirmButton = {
                TextButton(onClick = { confirmRemove = false; model.removeVault(active) }) { Text("Remove") }
            },
            dismissButton = { TextButton(onClick = { confirmRemove = false }) { Text("Cancel") } },
        )
    }
}
